//! The CSG cut command: subtract a stroke strip from every body it
//! crosses.
//!
//! Cuts only go **through**: subtracting the strip must separate the body
//! into components, swallow it entirely, or open a body the stroke passes
//! right through (a ring cut on one side) — a stroke that ends inside
//! the material is rejected, so no notches or microscopic thin features
//! can appear.
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
use gradiance_domain::node::NodeAttachment;
use gradiance_domain::shape::{CsgOp, ShapeDef};
use gradiance_geometry::contours::{Contours, point_in_ring};
use gradiance_geometry::polygonize::polygonize_components_on_axes;
use gradiance_geometry::sdf;
use gradiance_scene::{BodyRecord, JointRecord};
use gradiance_units::Displacement;

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
    replacement: Replacement,
}

/// What one stroke does: the bodies it severs or swallows, and the joints
/// on them.
#[derive(Debug, Default)]
struct CutStage {
    outcomes: Vec<CutOutcome>,
    /// Affected joints, each once: the pre-cut record, and the rewired
    /// definition (`None` = the joint is deleted by this cut). A joint
    /// whose two bodies are both cut has both ends rewired in one
    /// definition.
    joint_changes: Vec<(JointRecord, Option<JointDef>)>,
}

/// Cuts every body crossed by the stroke from `a` to `b`.
#[derive(Debug)]
pub struct CutCommand {
    /// Stroke start, world space.
    pub a: Vec2,
    /// Stroke end, world space.
    pub b: Vec2,
    /// Stroke width, world metres.
    pub width: f32,
}

impl CutCommand {
    /// Builds a cut along the world-space segment `a`→`b`.
    pub fn new(a: Vec2, b: Vec2, width: f32) -> Self {
        Self { a, b, width }
    }

    /// The cutting strip in a body's local frame: a box along the stroke,
    /// extended by one width so the stroke's round-off never leaves a film
    /// at its ends. Only its bounds are used (the pre-check).
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

    /// The stroke frame: its midpoint (world) and direction angle. The
    /// stroke runs along this frame's `x` axis.
    fn frame(&self) -> (Vec2, f32) {
        let dir = self.b - self.a;
        let angle = if dir.length() > 1e-6 {
            dir.to_angle()
        } else {
            0.0
        };
        ((self.a + self.b) / 2.0, angle)
    }

    /// The connected pieces left when the strip is cut out of `original`,
    /// in the body's local frame.
    ///
    /// Contoured in the stroke frame, on a lattice with a sample row on the
    /// stroke's centre line (`polygonize_components_on_axes`): a slit
    /// thinner than the body's contour cell still severs it. Contoured in
    /// the body frame, such a slit fell between two sample rows wherever
    /// the body sat, and the body ignored the cut (brief J-2).
    fn pieces(&self, original: &BodyRecord) -> Vec<Contours> {
        let (mid, angle) = self.frame();
        let to_stroke = Vec2::from_angle(-angle);
        let len = self.a.distance(self.b);
        let tree = ShapeDef::Csg {
            op: CsgOp::Subtract,
            lhs: Box::new(ShapeDef::Placed {
                pos: to_stroke.rotate(original.pose.pos - mid),
                rot: original.pose.rot - angle,
                shape: Box::new(original.shape.clone()),
            }),
            rhs: Box::new(ShapeDef::Box {
                width: len + self.width,
                height: self.width,
            }),
        };
        // Stroke frame -> body frame.
        let rot = Vec2::from_angle(angle);
        let to_body = Vec2::from_angle(-original.pose.rot);
        let map = |p: Vec2| to_body.rotate(mid + rot.rotate(p) - original.pose.pos);
        polygonize_components_on_axes(&tree)
            .into_iter()
            .map(|c| Contours {
                outline: c.outline.into_iter().map(map).collect(),
                holes: c
                    .holes
                    .into_iter()
                    .map(|h| h.into_iter().map(map).collect())
                    .collect(),
            })
            .collect()
    }

    /// Whether the stroke passes *through* the body's material: both of
    /// its ends lie outside it and it crosses it somewhere. Cutting a
    /// ring on one side opens it without severing it; a stroke that ends
    /// inside the material would leave a notch, so it does nothing.
    fn passes_through(&self, original: &BodyRecord) -> bool {
        let inv = Vec2::from_angle(-original.pose.rot);
        let local = |p: Vec2| inv.rotate(p - original.pose.pos);
        let inside = |p: Vec2| sdf::eval(&original.shape, local(p)) < 0.0;
        if inside(self.a) || inside(self.b) {
            return false;
        }
        let len = self.a.distance(self.b);
        let step = (self.width / 2.0).max(len / 4096.0).max(1e-4);
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (len / step).ceil() as usize;
        (1..n).any(|i| {
            #[expect(clippy::cast_precision_loss)]
            let t = i as f32 / n as f32;
            sdf::eval(&original.shape, local(self.a.lerp(self.b, t))) < -self.width / 4.0
        })
    }

    /// Computes what the stroke does to every body it crosses.
    fn stage(&self, world: &mut World) -> CutStage {
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
            let components = self.pieces(&original);

            // A cut severs (two or more pieces), swallows (none), or opens
            // a body it passes right through (one piece: a ring cut on one
            // side). A stroke that ends inside the material leaves the body
            // untouched (no notches, no microscopic thin features).
            // Deliberate notches are body booleans instead (subtract /
            // intersect / xor between bodies, `merge_cmd`).
            let replacement = match components.len() {
                0 => Replacement::Destroy,
                1 if !self.passes_through(&original) => continue,
                _ => Replacement::Split {
                    pieces: piece_records(&original, components),
                },
            };

            outcomes.push(CutOutcome {
                original,
                replacement,
            });
        }
        let joint_changes = rewire_joints(world, &outcomes);
        CutStage {
            outcomes,
            joint_changes,
        }
    }
}

/// Fresh polygon-leaf bodies for the connected `components` of a shape
/// in `original`'s local frame: each recentred on its centroid, posed at
/// that centroid in the world, and inheriting `original`'s properties,
/// appearance, depth, groups, field source, tracer, fluid and initial motion under a new id.
/// Shared by cuts and by booleans that split a body.
pub(crate) fn piece_records(original: &BodyRecord, components: Vec<Contours>) -> Vec<BodyRecord> {
    components
        .into_iter()
        .map(|mut piece| {
            let centroid = piece.recenter();
            BodyRecord {
                id: StableId::new(),
                pose: gradiance_core::units::PosRot {
                    pos: original.pose.pos + Vec2::from_angle(original.pose.rot).rotate(centroid),
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
                groups: original.groups.clone(),
                // Pieces inherit the parent's field source and tracer.
                field: original.field,
                tracer: original.tracer,
                fluid: original.fluid,
                motion: original.motion,
                optics: original.optics,
                lifetime: original.lifetime,
                surface: original.surface,
                // The image stays where it was on the page: the piece's
                // origin moved to its centroid.
                texture: original.texture.clone().map(|mut t| {
                    t.offset.0 -= centroid;
                    t
                }),
            }
        })
        .collect()
}

/// Replaces the body `original` with `pieces`: despawns it and spawns
/// each piece.
///
/// Physics continuity (Algodoo 7.5): pieces of a moving body keep
/// flying — each inherits `v + ω × r` (r = its centroid offset) and the
/// shared spin. Read live at apply time and never recorded: velocities
/// are simulation state, not authored state, exactly like the grab
/// spring's writes.
pub(crate) fn replace_with_pieces(
    world: &mut World,
    original: &BodyRecord,
    pieces: &[BodyRecord],
) -> Result<(), CommandError> {
    let entity = resolve(world, original.id)?;
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
            let r = piece.pose.pos - original.pose.pos;
            world
                .entity_mut(spawned)
                .insert((LinearVelocity(v + omega * r.perp()), AngularVelocity(omega)));
        }
    }
    Ok(())
}

/// Reattaches each joint on a cut body to the piece containing its anchor
/// (remapping the anchor into the piece frame), or deletes it when its body
/// was swallowed or no piece holds the anchor. Each joint is staged once,
/// so a joint between two cut bodies has both ends rewired.
fn rewire_joints(
    world: &mut World,
    outcomes: &[CutOutcome],
) -> Vec<(JointRecord, Option<JointDef>)> {
    let ids: Vec<StableId> = outcomes.iter().map(|o| o.original.id).collect();
    crate::joint_cmd::joints_referencing(world, &ids)
        .into_iter()
        .map(|(_, record)| {
            let def = outcomes
                .iter()
                .try_fold(record.def.clone(), |def, outcome| {
                    if def.body_a == outcome.original.id {
                        rewire_end(def, outcome, true)
                    } else if def.body_b == Some(outcome.original.id) {
                        rewire_end(def, outcome, false)
                    } else {
                        Some(def)
                    }
                });
            (record, def)
        })
        .collect()
}

/// One end of `def` (body a when `on_a`) moved onto the piece of `outcome`
/// that contains its anchor, or `None` when no piece does.
fn rewire_end(mut def: JointDef, outcome: &CutOutcome, on_a: bool) -> Option<JointDef> {
    let Replacement::Split { pieces } = &outcome.replacement else {
        return None;
    };
    let original = &outcome.original;
    let anchor = if on_a {
        def.anchor_a.value()
    } else {
        def.anchor_b.value()
    };
    pieces.iter().find_map(|piece| {
        // The piece's centroid in the original body's local frame.
        let offset =
            Vec2::from_angle(-original.pose.rot).rotate(piece.pose.pos - original.pose.pos);
        if !shape_contains(&piece.shape, anchor - offset) {
            return None;
        }
        if on_a {
            def.body_a = piece.id;
            def.anchor_a = Displacement(anchor - offset);
        } else {
            def.body_b = Some(piece.id);
            def.anchor_b = Displacement(anchor - offset);
        }
        Some(def.clone())
    })
}

/// Even-odd containment against a polygon-leaf shape.
pub(crate) fn shape_contains(shape: &ShapeDef, p: Vec2) -> bool {
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

pub(crate) fn aabbs_overlap(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}

/// Rewires or deletes each behavior node attached to a cut body — the
/// joints' rule, for nodes: a split re-attaches the node to the piece
/// that contains its offset (the pieces share the original's rotation,
/// so the node's world direction never moves), and a node whose piece was
/// swallowed, or whose body was destroyed, goes with its body. Undo is
/// the snapshot, as ever.
fn rewire_cut_nodes(world: &mut World, outcomes: &[CutOutcome]) {
    let ids: Vec<StableId> = outcomes.iter().map(|o| o.original.id).collect();
    for (entity, record) in crate::spawn::nodes_attached(world, &ids) {
        let offset = record.attachment.offset.value();
        let Some(outcome) = record
            .attachment
            .target
            .and_then(|t| outcomes.iter().find(|o| o.original.id == t))
        else {
            continue;
        };
        let new = match &outcome.replacement {
            Replacement::Split { pieces } => pieces.iter().find_map(|piece| {
                // The piece's centroid in the original's frame, as
                // `rewire_end` measures it.
                let local = Vec2::from_angle(-outcome.original.pose.rot)
                    .rotate(piece.pose.pos - outcome.original.pose.pos);
                shape_contains(&piece.shape, offset - local).then(|| (piece.id, offset - local))
            }),
            Replacement::Destroy => None,
        };
        match new {
            Some((body, offset)) => {
                if let Some(mut live) = world.get_mut::<NodeAttachment>(entity) {
                    *live = NodeAttachment::to(body, offset);
                }
            }
            None => {
                world.despawn(entity);
            }
        }
    }
}

impl GameCommand for CutCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let stage = self.stage(world);
        apply_stage(world, &stage)
    }

    fn name(&self) -> &'static str {
        crate::intent::name::CUT
    }
}

/// Erases a brush stroke from every body it touches (brief S-16): the
/// capsule union along `line`, `width` wide, is subtracted from each body.
/// Unlike a cut, an erase may end inside the material: a notch is what an
/// eraser leaves. Pieces follow the cut's rules (fresh polygon-leaf bodies,
/// joints and nodes rewired, a swallowed body removed).
#[derive(Debug)]
pub struct EraseCommand {
    /// The stroke, world space.
    pub line: Vec<Vec2>,
    /// The brush width, world metres.
    pub width: f32,
}

impl EraseCommand {
    /// Computes what the stroke erases from every body it touches.
    fn stage(&self, world: &mut World) -> CutStage {
        let Some(brush) = gradiance_geometry::sketch::brush_shape(&self.line, self.width / 2.0)
        else {
            return CutStage::default();
        };
        let bodies: Vec<Entity> = {
            let mut query = world.query_filtered::<Entity, With<Body>>();
            query.iter(world).collect()
        };
        let mut outcomes = Vec::new();
        for entity in bodies {
            let Some(original) = BodyRecord::capture(world, entity) else {
                continue;
            };
            if original.shape.contains_half_plane() {
                continue;
            }
            let pose = original.pose;
            // The brush in the body's frame.
            let inv = Vec2::from_angle(-pose.rot);
            let local_brush = ShapeDef::Placed {
                pos: inv.rotate(brush.pos - pose.pos),
                rot: brush.rot - pose.rot,
                shape: Box::new(brush.shape.clone()),
            };
            if !aabbs_overlap(sdf::aabb(&original.shape), sdf::aabb(&local_brush))
                || !touches(&original, &brush.line, brush.radius)
            {
                continue;
            }
            let tree = ShapeDef::Csg {
                op: CsgOp::Subtract,
                lhs: Box::new(original.shape.clone()),
                rhs: Box::new(local_brush),
            };
            let components = gradiance_geometry::polygonize::polygonize_components(&tree);
            let replacement = if components.is_empty() {
                Replacement::Destroy
            } else {
                Replacement::Split {
                    pieces: piece_records(&original, components),
                }
            };
            outcomes.push(CutOutcome {
                original,
                replacement,
            });
        }
        let joint_changes = rewire_joints(world, &outcomes);
        CutStage {
            outcomes,
            joint_changes,
        }
    }
}

/// Whether a brush of `radius` along `line` (world) reaches `body`'s
/// material: some sample of the centre line, a quarter radius apart, lies
/// within `radius` of the inside.
fn touches(body: &BodyRecord, line: &[Vec2], radius: f32) -> bool {
    let inv = Vec2::from_angle(-body.pose.rot);
    let local = |p: Vec2| inv.rotate(p - body.pose.pos);
    let step = (radius / 4.0).max(1e-4);
    line.windows(2).any(|w| {
        let len = w[0].distance(w[1]);
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (len / step).ceil().max(1.0) as usize;
        (0..=n).any(|i| {
            #[expect(clippy::cast_precision_loss)]
            let t = i as f32 / n as f32;
            sdf::eval(&body.shape, local(w[0].lerp(w[1], t))) < radius
        })
    })
}

impl GameCommand for EraseCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let stage = self.stage(world);
        apply_stage(world, &stage)
    }

    fn name(&self) -> &'static str {
        crate::intent::name::ERASE
    }
}

/// Applies a staged cut or erase: each body replaced by its pieces or
/// removed, then its joints and nodes rewired. `NoEffect` when the stroke
/// touched nothing.
fn apply_stage(world: &mut World, stage: &CutStage) -> Result<(), CommandError> {
    if stage.outcomes.is_empty() {
        return Err(CommandError::NoEffect);
    }
    for outcome in &stage.outcomes {
        match &outcome.replacement {
            Replacement::Split { pieces } => {
                replace_with_pieces(world, &outcome.original, pieces)?;
            }
            Replacement::Destroy => {
                let entity = resolve(world, outcome.original.id)?;
                world.despawn(entity);
            }
        }
    }
    apply_joint_changes(world, &stage.joint_changes)?;
    rewire_cut_nodes(world, &stage.outcomes);
    Ok(())
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
