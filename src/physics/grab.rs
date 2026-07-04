//! Mouse spring: engine-agnostic API for the drag tool's physical grab.

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
