//! The CSG cut command: subtract a stroke strip from every body it
//! crosses.
//!
//! Cuts only **sever**: subtracting the strip must separate the body
//! into components (or swallow it entirely) — partial strokes are
//! rejected, so no notches or microscopic thin features can appear.
//! Pieces become fresh polygon-leaf bodies (connectivity is a
//! topological property no field boolean can express, so splitting is
//! where discretization is inherent).
//!
//! Joints referencing a split body reattach to the piece containing
//! their anchor (anchor remapped into the piece's frame) and are deleted
//! when no piece contains it.

use crate::{CommandError, GameCommand, resolve};
use avian2d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::joint::JointDef;
use gradiance_domain::shape::{CsgOp, ShapeDef};
use gradiance_geometry::contours::point_in_ring;
use gradiance_geometry::polygonize::polygonize_components;
use gradiance_geometry::sdf;
use gradiance_scene::{BodyRecord, JointRecord};

/// What happened to one cut body.
#[derive(Debug)]
enum Replacement {
    /// Severed: the body is replaced by fresh polygon-leaf pieces.
    Split {
        /// Piece records (ids minted at staging, stable across redo).
        pieces: Vec<BodyRecord>,
    },
    /// Nothing remained (the strip swallowed the body).
    Destroy,
}

/// Everything recorded about one affected body.
#[derive(Debug)]
struct CutOutcome {
    /// The body's pre-cut state (undo restores it verbatim).
    original: BodyRecord,
    /// Affected joints: the pre-cut record, and the rewired definition
    /// (`None` = the joint is deleted by this cut).
    joint_changes: Vec<(JointRecord, Option<JointDef>)>,
    replacement: Replacement,
}

/// Cuts every body crossed by the stroke from `a` to `b`.
#[derive(Debug)]
pub struct CutCommand {
    /// Stroke start, world space.
    pub a: Vec2,
    /// Stroke end, world space.
    pub b: Vec2,
    /// Stroke width, world pixels.
    pub width: f32,
}

impl CutCommand {
    /// Builds a cut along the world-space segment `a`→`b`.
    pub fn new(a: Vec2, b: Vec2, width: f32) -> Self {
        Self { a, b, width }
    }

    /// The cutting strip in a body's local frame: a box along the stroke,
    /// extended by one width so the stroke's round-off never leaves a film
    /// at its ends.
    fn local_strip(&self, pose: &gradiance_core::units::PosRot) -> ShapeDef {
        let inv = Vec2::from_angle(-pose.rot);
        let a = inv.rotate(self.a - pose.pos);
        let b = inv.rotate(self.b - pose.pos);
        let dir = b - a;
        let len = dir.length();
        ShapeDef::Placed {
            pos: (a + b) / 2.0,
            rot: if len > 1e-6 { dir.to_angle() } else { 0.0 },
            shape: Box::new(ShapeDef::Box {
                width: len + self.width,
                height: self.width,
            }),
        }
    }

    /// Computes what the stroke does to every body it crosses.
    fn stage(&self, world: &mut World) -> Vec<CutOutcome> {
        let mut outcomes = Vec::new();
        let bodies: Vec<Entity> = {
            let mut query = world.query_filtered::<Entity, With<Body>>();
            query.iter(world).collect()
        };
        for entity in bodies {
            let Some(original) = BodyRecord::capture(world, entity) else {
                continue;
            };
            // The infinite ground is not cuttable.
            if original.shape.contains_half_plane() {
                continue;
            }
            let strip = self.local_strip(&original.pose);
            if !aabbs_overlap(sdf::aabb(&original.shape), sdf::aabb(&strip)) {
                continue;
            }
            let cut_tree = ShapeDef::Csg {
                op: CsgOp::Subtract,
                lhs: Box::new(original.shape.clone()),
                rhs: Box::new(strip),
            };
            let components = polygonize_components(&cut_tree);

            // Cuts only *sever*: a stroke that fails to pass fully through
            // leaves the body untouched (no notches, no microscopic thin
            // features). CSG modeling (boolean ops between bodies) is a
            // separate planned feature.
            let replacement = if components.is_empty() {
                Replacement::Destroy
            } else if components.len() == 1 {
                continue;
            } else {
                let pieces = components
                    .into_iter()
                    .map(|mut piece| {
                        let centroid = piece.recenter();
                        BodyRecord {
                            id: StableId::new(),
                            pose: gradiance_core::units::PosRot {
                                pos: original.pose.pos
                                    + Vec2::from_angle(original.pose.rot).rotate(centroid),
                                rot: original.pose.rot,
                            },
                            shape: ShapeDef::Polygon {
                                outline: piece.outline,
                                holes: piece.holes,
                            },
                            physics: original.physics,
                            appearance: original.appearance,
                            depth: original.depth,
                            layers: None,
                            // A severed fragment is no longer the profile the
                            // sketch described, so its constraints would name
                            // geometry that no longer exists. Cutting a
                            // sketched body deliberately drops the sketch and
                            // leaves a plain polygon body.
                            sketch: None,
                            groups: original.groups.clone(),
                            // Cut pieces inherit the parent's field source
                            // and tracer.
                            field: original.field,
                            tracer: original.tracer,
                        }
                    })
                    .collect();
                Replacement::Split { pieces }
            };

            let joint_changes = match &replacement {
                Replacement::Destroy => crate::joint_cmd::joints_referencing(world, &[original.id])
                    .into_iter()
                    .map(|(_, record)| (record, None))
                    .collect(),
                Replacement::Split { pieces } => rewire_joints(world, &original, pieces),
            };

            outcomes.push(CutOutcome {
                original,
                joint_changes,
                replacement,
            });
        }
        outcomes
    }
}

/// Reattaches each joint on a split body to the piece containing its
/// anchor (remapping the anchor into the piece frame), or deletes it.
fn rewire_joints(
    world: &mut World,
    original: &BodyRecord,
    pieces: &[BodyRecord],
) -> Vec<(JointRecord, Option<JointDef>)> {
    // Piece centroids in the original body's local frame.
    let piece_offsets: Vec<Vec2> = pieces
        .iter()
        .map(|p| Vec2::from_angle(-original.pose.rot).rotate(p.pose.pos - original.pose.pos))
        .collect();

    crate::joint_cmd::joints_referencing(world, &[original.id])
        .into_iter()
        .map(|(_, record)| {
            let on_body_a = record.def.body_a == original.id;
            let anchor = if on_body_a {
                record.def.anchor_a
            } else {
                record.def.anchor_b
            };
            let mut rewired = None;
            for (piece, offset) in pieces.iter().zip(piece_offsets.iter()) {
                if shape_contains(&piece.shape, anchor - *offset) {
                    let mut def = record.def.clone();
                    if on_body_a {
                        def.body_a = piece.id;
                        def.anchor_a = anchor - *offset;
                    } else {
                        def.body_b = Some(piece.id);
                        def.anchor_b = anchor - *offset;
                    }
                    rewired = Some(def);
                    break;
                }
            }
            (record, rewired)
        })
        .collect()
}

/// Even-odd containment against a polygon-leaf shape.
fn shape_contains(shape: &ShapeDef, p: Vec2) -> bool {
    let ShapeDef::Polygon { outline, holes } = shape else {
        return false;
    };
    let mut inside = point_in_ring(p, outline);
    for hole in holes {
        if point_in_ring(p, hole) {
            inside = !inside;
        }
    }
    inside
}

fn aabbs_overlap(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}

impl GameCommand for CutCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let outcomes = self.stage(world);
        if outcomes.is_empty() {
            return Err(CommandError::NoEffect);
        }
        for outcome in &outcomes {
            match &outcome.replacement {
                Replacement::Split { pieces } => {
                    let entity = resolve(world, outcome.original.id)?;
                    // Physics continuity (Algodoo 7.5): pieces of a severed
                    // moving body keep flying — each inherits `v + ω × r`
                    // (r = its centroid offset) and the shared spin. Read
                    // live at apply time and never recorded: velocities are
                    // simulation state, not authored state, exactly like
                    // the grab spring's writes.
                    let motion = world.get::<LinearVelocity>(entity).copied().map(|lv| {
                        (
                            lv.0,
                            world.get::<AngularVelocity>(entity).map_or(0.0, |a| a.0),
                        )
                    });
                    world.despawn(entity);
                    for piece in pieces {
                        let spawned = piece.spawn(world);
                        if let Some((v, omega)) = motion {
                            let r = piece.pose.pos - outcome.original.pose.pos;
                            world.entity_mut(spawned).insert((
                                LinearVelocity(v + omega * r.perp()),
                                AngularVelocity(omega),
                            ));
                        }
                    }
                    apply_joint_changes(world, &outcome.joint_changes)?;
                }
                Replacement::Destroy => {
                    let entity = resolve(world, outcome.original.id)?;
                    world.despawn(entity);
                    apply_joint_changes(world, &outcome.joint_changes)?;
                }
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::CUT
    }
}

pub(crate) fn apply_joint_changes(
    world: &mut World,
    changes: &[(JointRecord, Option<JointDef>)],
) -> Result<(), CommandError> {
    for (record, new_def) in changes {
        let entity = resolve(world, record.id)?;
        match new_def {
            Some(def) => {
                if let Some(mut live) = world.get_mut::<JointDef>(entity) {
                    *live = def.clone();
                }
            }
            None => {
                world.despawn(entity);
            }
        }
    }
    Ok(())
}
