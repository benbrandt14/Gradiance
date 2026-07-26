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
                // Reverse in avian's constraint frame (rest basis included, so
                // the reversal lines up with the real limit — the old code
                // omitted the basis and the motor just drove into the stop).
                // Both the angle and the reversal decision are pure, tested
                // domain functions.
                let rel = gradiance_domain::joint::hinge_limit_angle(
                    rot(a),
                    rot_b,
                    def.rest_rot_a,
                    def.rest_rot_b,
                );
                if let Some(v) = gradiance_domain::joint::oscillate_target(
                    rel,
                    *min,
                    *max,
                    m.target_velocity.value(),
                    ANGLE_BUFFER,
                ) {
                    joint.motor.target_velocity = v;
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
                if let Some(v) = gradiance_domain::joint::oscillate_target(
                    t,
                    *min,
                    *max,
                    m.target_velocity.value(),
                    TRANSLATION_BUFFER,
                ) {
                    joint.motor.target_velocity = v;
                }
            }
            _ => {}
        }
    }
}
