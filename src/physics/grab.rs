//! Mouse spring & twist: the transient *physical* manipulation state.
//!
//! The drag tool's grab (a velocity spring toward the cursor) and the
//! play-mode rotate's twist (a velocity servo toward a target angle) are
//! physical interactions — never commands, never undoable, matching Algodoo.

use avian2d::prelude::*;
use bevy::prelude::*;

/// Velocity gain toward the target (per second).
const SPRING_GAIN: f32 = 12.0;
/// Maximum induced speed (px/s).
const MAX_SPEED: f32 = 6_000.0;
/// Angular velocity damping factor per update while grabbed.
const ANGULAR_DAMP: f32 = 0.98;

/// An active physical grab.
#[derive(Debug, Clone, Copy)]
pub struct Grab {
    /// Grabbed body.
    pub entity: Entity,
    /// Grip point in body-local space (fixes the classic latch-to-center
    /// bug: the body is pulled by where it was grabbed).
    pub local_point: Vec2,
    /// Where the cursor wants the grip point, world space.
    pub target: Vec2,
}

/// The drag tool's current grab, if any.
#[derive(Resource, Default, Debug)]
pub struct MouseSpring(pub Option<Grab>);

/// Pulls the grabbed body's grip point toward the target with a velocity
/// spring.
pub fn apply_mouse_spring(
    spring: Res<MouseSpring>,
    mut bodies: Query<(&Transform, &mut LinearVelocity, &mut AngularVelocity)>,
) {
    let Some(grab) = spring.0 else {
        return;
    };
    let Ok((transform, mut linear, mut angular)) = bodies.get_mut(grab.entity) else {
        return;
    };
    let world_grip = transform
        .compute_affine()
        .transform_point3(grab.local_point.extend(0.0))
        .truncate();
    let pull = (grab.target - world_grip) * SPRING_GAIN;
    linear.0 = pull.clamp_length_max(MAX_SPEED);
    angular.0 *= ANGULAR_DAMP;
}

/// Spin gain toward the target angle (per second).
const TWIST_GAIN: f32 = 12.0;
/// Maximum induced spin (rad/s).
const MAX_SPIN: f32 = 40.0;

/// An active physical twist: drive one body's rotation toward a target
/// angle by angular velocity, leaving translation to the solver — the
/// pivot is *not* fixed, so a resting body lifts its opposing edge
/// instead of teleport-rotating (feedback 2.6).
#[derive(Debug, Clone, Copy)]
pub struct Twist {
    /// Twisted body.
    pub entity: Entity,
    /// Desired world rotation, radians.
    pub target_rot: f32,
}

/// The play-mode rotate gesture's current twists (one per selected body),
/// empty when no twist is active.
#[derive(Resource, Default, Debug)]
pub struct MouseTwist(pub Vec<Twist>);

/// Servos each twisted body's angular velocity toward its target angle.
pub fn apply_mouse_twist(
    twist: Res<MouseTwist>,
    mut bodies: Query<(&Transform, &mut AngularVelocity)>,
) {
    for t in &twist.0 {
        let Ok((transform, mut angular)) = bodies.get_mut(t.entity) else {
            continue;
        };
        let rot = crate::core::units::PosRot::from_transform(transform).rot;
        let err = wrap_pi(t.target_rot - rot);
        angular.0 = (err * TWIST_GAIN).clamp(-MAX_SPIN, MAX_SPIN);
    }
}

/// Wraps an angle difference into `(-π, π]` so the servo always takes the
/// short way around.
fn wrap_pi(angle: f32) -> f32 {
    let wrapped = angle.rem_euclid(std::f32::consts::TAU);
    if wrapped > std::f32::consts::PI {
        wrapped - std::f32::consts::TAU
    } else {
        wrapped
    }
}
