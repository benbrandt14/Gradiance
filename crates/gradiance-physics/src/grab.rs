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

/// Twist proportional gain (angular acceleration per radian of error).
const TWIST_KP: f32 = 150.0;
/// Twist damping gain (≈ 2·√KP, near-critical damping).
const TWIST_KD: f32 = 24.0;
/// Angular-acceleration clamp (rad/s²).
const MAX_TWIST_ACCEL: f32 = 600.0;

/// An active physical twist: drive one body's rotation toward a target
/// angle by *torque*, leaving everything else to the solver — the pivot is
/// not fixed (a resting body lifts its opposing edge instead of
/// teleport-rotating, feedback 2.6), and constraints always win: a body
/// held by a prismatic joint or a rotation lock does not rotate, no matter
/// how hard the gesture pulls (a velocity write would punch through).
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

/// PD-servos each twisted body toward its target angle with a one-shot
/// torque (cleared by avian after the step), so joints and rotation locks
/// are respected — the solver, not the gesture, has the last word.
pub fn apply_mouse_twist(
    twist: Res<MouseTwist>,
    mut bodies: Query<(&ComputedAngularInertia, Forces)>,
) {
    for t in &twist.0 {
        let Ok((inertia, mut forces)) = bodies.get_mut(t.entity) else {
            continue;
        };
        // Pose/velocity read through `Forces` itself — it holds write access
        // to the velocity components, so separate query terms would conflict.
        let err = gradiance_geometry::wrap_angle(t.target_rot - forces.rotation().as_radians());
        let accel = (err * TWIST_KP - forces.angular_velocity() * TWIST_KD)
            .clamp(-MAX_TWIST_ACCEL, MAX_TWIST_ACCEL);
        forces.apply_torque(inertia.value() * accel);
    }
}
