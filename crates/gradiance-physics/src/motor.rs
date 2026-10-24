//! Oscillating motors: flip the native motor's target velocity at the
//! joint limits.
//!
//! Everything else about motors is handled natively by the engine
//! (velocity tracking, max force, damping); this system only implements
//! the Algodoo "back and forth" behavior on top.
//!
//! The angle and reversal maths are pure domain functions and are untouched by
//! the move to a 3D engine — the payoff of having put them in `domain`.

use crate::joint_sync::DerivedJoint;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_core::units::PlaneFrame;
use gradiance_domain::joint::{JointDef, JointKind};

/// Angular/linear buffer before a limit at which the motor reverses.
const ANGLE_BUFFER: f32 = 0.05;
const TRANSLATION_BUFFER: f32 = 0.02;

/// Reverses oscillating motors at their limits.
pub fn drive_oscillating_motors(
    index: Res<IdIndex>,
    defs: Query<(Entity, &JointDef)>,
    transforms: Query<&GlobalTransform>,
    derived: Query<&DerivedJoint>,
    mut joints: Query<&mut ImpulseJoint>,
) {
    let plane = PlaneFrame::XY;
    for (entity, def) in &defs {
        let Some(a) = index.entity(def.body_a) else {
            continue;
        };
        // Projected through the plane rather than pulled out of the quaternion
        // by hand, so the 2D-authoring seam has no exception.
        let rot = |e: Entity| {
            transforms
                .get(e)
                .map_or(0.0, |t| plane.pose(&t.compute_transform()).rot)
        };
        let pos = |e: Entity| {
            transforms
                .get(e)
                .map(|t| plane.project(t.translation()).0)
                .unwrap_or_default()
        };

        match &def.kind {
            JointKind::Hinge {
                limits: Some([min, max]),
                motor: Some(m),
            } if m.oscillate && m.enabled => {
                let Ok(mut joint) = derived_joint(&derived, &mut joints, entity) else {
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
                    set_motor_velocity(&mut joint, JointAxis::AngX, v, m.damping);
                }
            }
            JointKind::Slider {
                axis,
                limits: Some([min, max]),
                motor: Some(m),
            } if m.oscillate && m.enabled => {
                let Ok(mut joint) = derived_joint(&derived, &mut joints, entity) else {
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
                    set_motor_velocity(&mut joint, JointAxis::LinX, v, m.damping);
                }
            }
            _ => {}
        }
    }
}

/// The engine joint derived from an authored joint entity.
fn derived_joint<'a>(
    derived: &Query<&DerivedJoint>,
    joints: &'a mut Query<&mut ImpulseJoint>,
    authored: Entity,
) -> Result<Mut<'a, ImpulseJoint>, ()> {
    let link = derived.get(authored).map_err(|_| ())?;
    joints.get_mut(link.0).map_err(|_| ())
}

/// Retargets one motor axis, leaving every other joint parameter alone.
fn set_motor_velocity(joint: &mut ImpulseJoint, axis: JointAxis, velocity: f32, damping: f32) {
    joint
        .data
        .as_mut()
        .set_motor_velocity(axis, velocity, damping);
}
