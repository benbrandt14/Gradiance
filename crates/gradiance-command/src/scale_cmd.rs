//! Scaling bodies about a pivot in a chosen frame.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance_geometry::scale::{BodyScale, scale_point};
use gradiance_units::{Angle, Displacement};
use std::collections::HashMap;

/// Minimum allowed scale factor magnitude.
pub const MIN_SCALE_FACTOR: f32 = 0.01;

/// Scales a set of bodies about `pivot` along the axes of a frame rotated
/// by `frame_rot` (0 = world/global axes; a body's rotation = its local
/// axes). Positions scale exactly; shapes scale parametrically when
/// representable and polygonize exactly otherwise.
///
/// One negative factor **mirrors** ([`BodyScale`]): each body turns to its
/// reflected rotation and its shape reflects across one local axis, so a
/// box stays a box and a CSG body stays a tree; mirroring twice gives the
/// scene back. Whatever the factors, the joints and behaviour nodes on a
/// scaled body go through the same body-local map, so they stay on the
/// material they were drawn on (brief N-9).
#[derive(Debug)]
pub struct ScaleCommand {
    /// Bodies to scale.
    pub targets: Vec<StableId>,
    /// Fixed point of the scale, world space.
    pub pivot: Vec2,
    /// Frame rotation in radians (axes along which factors apply).
    pub frame_rot: f32,
    /// Per-axis scale factors in the frame.
    pub factors: Vec2,
}

impl ScaleCommand {
    /// Builds a scale command.
    pub fn new(targets: Vec<StableId>, pivot: Vec2, frame_rot: f32, factors: Vec2) -> Self {
        Self {
            targets,
            pivot,
            frame_rot,
            factors,
        }
    }
}

impl GameCommand for ScaleCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.targets.is_empty()
            || self.factors.x.abs() < MIN_SCALE_FACTOR
            || self.factors.y.abs() < MIN_SCALE_FACTOR
        {
            return Err(CommandError::NoEffect);
        }
        // Resolve and validate everything before mutating anything.
        let mut staged = Vec::with_capacity(self.targets.len());
        for &id in &self.targets {
            let entity = resolve(world, id)?;
            let shape = world
                .get::<ShapeDef>(entity)
                .ok_or(CommandError::MissingEntity(id))?;
            let transform = *world
                .get::<Transform>(entity)
                .ok_or(CommandError::MissingEntity(id))?;
            let pose = PosRot::from_transform(&transform);

            let map = BodyScale::new(
                pose.rot,
                self.frame_rot,
                self.factors,
                *shape == ShapeDef::HalfPlane,
            );
            let new_shape = map.shape(shape);
            new_shape.validate()?;
            let new_pos = scale_point(pose.pos, self.pivot, self.frame_rot, self.factors);
            staged.push((id, entity, map, new_shape, new_pos));
        }
        let maps: HashMap<StableId, BodyScale> =
            staged.iter().map(|(id, _, map, ..)| (*id, *map)).collect();
        let joints = joint_edits(world, &maps, |p| {
            scale_point(p, self.pivot, self.frame_rot, self.factors)
        });
        let nodes = node_edits(world, &maps, &staged);

        for (_, entity, map, new_shape, new_pos) in staged {
            let mut entity_mut = world.entity_mut(entity);
            if let Some(mut s) = entity_mut.get_mut::<ShapeDef>() {
                *s = new_shape;
            }
            if let Some(mut t) = entity_mut.get_mut::<Transform>() {
                PosRot {
                    pos: new_pos,
                    rot: map.rot,
                }
                .apply_to(&mut t);
            }
        }
        for (entity, def) in joints {
            if let Some(mut d) = world.get_mut::<JointDef>(entity) {
                *d = def;
            }
        }
        for (entity, attachment, kind, pos) in nodes {
            let mut entity_mut = world.entity_mut(entity);
            if let Some(mut a) = entity_mut.get_mut::<NodeAttachment>() {
                *a = attachment;
            }
            if let Some(mut k) = entity_mut.get_mut::<NodeKind>()
                && *k != kind
            {
                *k = kind;
            }
            if let Some(mut t) = entity_mut.get_mut::<Transform>() {
                t.translation.x = pos.x;
                t.translation.y = pos.y;
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        // One negative factor turns the selection over: the step reads
        // "Mirror", whether a menu row or a handle dragged past the pivot
        // made it.
        if (self.factors.x < 0.0) == (self.factors.y < 0.0) {
            crate::intent::name::SCALE
        } else {
            crate::intent::name::MIRROR
        }
    }
}

/// The joints with an end on a scaled body, rewritten: each anchor on a
/// scaled body through its map, a world pin of a scaled body A through the
/// world scale (`world`; it moves with its body), a slider axis on a scaled
/// body A through its map, and each rest rotation the way its body turned.
///
/// When **every** body of a joint is mirrored (a world pin counts as its
/// body A's), the whole joint is reflected: relative angles change sign, so
/// a hinge's limits become `[−max, −min]` and its motor turns the other
/// way. A joint with one mirrored end keeps its limits and motor.
fn joint_edits(
    world: &mut World,
    maps: &HashMap<StableId, BodyScale>,
    world_point: impl Fn(Vec2) -> Vec2,
) -> Vec<(Entity, JointDef)> {
    let mut query = world.query::<(Entity, &JointDef)>();
    query
        .iter(world)
        .filter_map(|(entity, def)| {
            let map_a = maps.get(&def.body_a);
            let map_b = def.body_b.and_then(|b| maps.get(&b));
            if map_a.is_none() && map_b.is_none() {
                return None;
            }
            let mut new = def.clone();
            if let Some(map) = map_a {
                new.anchor_a = Displacement(map.point(def.anchor_a.value()));
                new.rest_rot_a = Angle(map.rotation(def.rest_rot_a.value()));
                if def.body_b.is_none() {
                    new.anchor_b = Displacement(world_point(def.anchor_b.value()));
                }
                if let JointKind::Slider { axis, .. } = &mut new.kind {
                    *axis = map.unit(*axis);
                }
            }
            if let Some(map) = map_b {
                new.anchor_b = Displacement(map.point(def.anchor_b.value()));
                new.rest_rot_b = Angle(map.rotation(def.rest_rot_b.value()));
            }
            let mirrored = |map: Option<&BodyScale>| map.is_some_and(BodyScale::mirrors);
            let whole = mirrored(map_a) && (def.body_b.is_none() || mirrored(map_b));
            if whole && let JointKind::Hinge { limits, motor } = &mut new.kind {
                if let Some([lo, hi]) = limits {
                    *limits = Some([Angle(-hi.value()), Angle(-lo.value())]);
                }
                if let Some(m) = motor {
                    m.target_velocity = gradiance_units::AngularVelocity(-m.target_velocity.value());
                }
            }
            (new != *def).then_some((entity, new))
        })
        .collect()
}

/// The behaviour nodes riding a scaled body, rewritten: the offset through
/// the body's map, a thruster's or laser's axis (in the body's frame) as a
/// direction through it, and the derived position recomputed so the
/// recorded step already shows the node where it will be drawn.
fn node_edits(
    world: &mut World,
    maps: &HashMap<StableId, BodyScale>,
    staged: &[(StableId, Entity, BodyScale, ShapeDef, Vec2)],
) -> Vec<(Entity, NodeAttachment, NodeKind, Vec2)> {
    let mut query =
        world.query_filtered::<(Entity, &NodeAttachment, &NodeKind), With<BehaviorNode>>();
    query
        .iter(world)
        .filter_map(|(entity, attachment, kind)| {
            let target = attachment.target?;
            let map = maps.get(&target)?;
            let pos = staged.iter().find(|s| s.0 == target).map(|s| s.4)?;
            let offset = map.point(attachment.offset.value());
            let mut kind = kind.clone();
            match &mut kind {
                NodeKind::Thruster(t) => t.angle = Angle(map.direction(t.angle.value())),
                NodeKind::Laser(l) => l.angle = Angle(map.direction(l.angle.value())),
                NodeKind::Tracer(_) => {}
            }
            Some((
                entity,
                NodeAttachment {
                    target: Some(target),
                    offset: Displacement(offset),
                },
                kind,
                pos + Vec2::from_angle(map.rot).rotate(offset),
            ))
        })
        .collect()
}
