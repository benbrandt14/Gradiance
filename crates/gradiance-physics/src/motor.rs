//! Oscillating motors: flip the native motor's target velocity at the
//! joint limits.
//!
//! Everything else about motors is handled natively by avian
//! (velocity tracking, max force, damping); this system only implements
//! the Algodoo "back and forth" behavior on top.

use avian2d::prelude::{PrismaticJoint, RevoluteJoint};
use bevy::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_domain::joint::{JointDef, JointKind};

/// Angular/linear buffer before a limit at which the motor reverses.
const ANGLE_BUFFER: f32 = 0.05;
const TRANSLATION_BUFFER: f32 = 0.02;

/// Reverses oscillating motors at their limits.
pub fn drive_oscillating_motors(
    index: Res<IdIndex>,
    defs: Query<(Entity, &JointDef)>,
    transforms: Query<&GlobalTransform>,
    mut revolutes: Query<&mut RevoluteJoint>,
    mut prismatics: Query<&mut PrismaticJoint>,
) {
    for (entity, def) in &defs {
        let Some(a) = index.entity(def.body_a) else {
            continue;
        };
        let rot = |e: Entity| {
            transforms.get(e).map_or(0.0, |t| {
                t.to_scale_rotation_translation()
                    .1
                    .to_euler(EulerRot::ZYX)
                    .0
            })
        };
        let pos = |e: Entity| {
            transforms
                .get(e)
                .map(|t| t.translation().truncate())
                .unwrap_or_default()
        };

        match &def.kind {
            JointKind::Hinge {
                limits: Some([min, max]),
                motor: Some(m),
            } if m.oscillate && m.enabled => {
                let Ok(mut joint) = revolutes.get_mut(entity) else {
                    continue;
                };
                let rot_b = def.body_b.and_then(|id| index.entity(id)).map_or(0.0, rot);
                let rel = gradiance_geometry::wrap_angle(rot_b - rot(a));
                let speed = m.target_velocity.abs();
                if rel >= max - ANGLE_BUFFER {
                    joint.motor.target_velocity = -speed;
                } else if rel <= min + ANGLE_BUFFER {
                    joint.motor.target_velocity = speed;
                }
            }
            JointKind::Slider {
                axis,
                limits: Some([min, max]),
                motor: Some(m),
            } if m.oscillate && m.enabled => {
                let Ok(mut joint) = prismatics.get_mut(entity) else {
                    continue;
                };
                let Some(b) = def.body_b.and_then(|id| index.entity(id)) else {
                    continue;
                };
                let dir = Vec2::from_angle(rot(a)).rotate(*axis);
                let anchor_a_world = pos(a) + Vec2::from_angle(rot(a)).rotate(def.anchor_a);
                let anchor_b_world = pos(b) + Vec2::from_angle(rot(b)).rotate(def.anchor_b);
                let t = (anchor_b_world - anchor_a_world).dot(dir);
                let speed = m.target_velocity.abs();
                if t >= max - TRANSLATION_BUFFER {
                    joint.motor.target_velocity = -speed;
                } else if t <= min + TRANSLATION_BUFFER {
                    joint.motor.target_velocity = speed;
                }
            }
            _ => {}
        }
    }
}
