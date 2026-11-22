//! A body's acceleration (brief S-2): the change in its velocity over the
//! last physics step.
//!
//! Derived, like contacts: [`BodyAcceleration`] is written after each step
//! by [`measure_accelerations`] (one `FixedPostUpdate` system, after
//! avian's step, only while the sim runs), never saved, never undone, and
//! read by the quantity catalog's `accel` row through
//! `queries::LiveBody`.

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;

/// The velocity a body ended the last step with, and its acceleration over
/// that step (m/s²). Zero on the first step a body is seen.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct BodyAcceleration {
    /// The velocity at the end of the last measured step (m/s).
    pub last_velocity: Vec2,
    /// `(v − last_velocity) / dt` over the last step (m/s²).
    pub value: Vec2,
}

impl BodyAcceleration {
    /// The next sample after a step of `dt` seconds that ended at `velocity`.
    #[must_use]
    pub fn after_step(self, velocity: Vec2, dt: f32) -> Self {
        let value = if dt > 0.0 {
            (velocity - self.last_velocity) / dt
        } else {
            self.value
        };
        Self {
            last_velocity: velocity,
            value: if value.is_finite() { value } else { Vec2::ZERO },
        }
    }
}

/// Measures every body's [`BodyAcceleration`] after the step. Cost `O(B)`,
/// allocation-free once every body has the component (a new body gets one
/// through a command, at rest).
pub fn measure_accelerations(
    mut commands: Commands,
    fixed: Res<Time<Fixed>>,
    physics: Res<Time<Physics>>,
    mut bodies: Query<(Entity, &LinearVelocity, Option<&mut BodyAcceleration>), With<Body>>,
) {
    let dt = fixed.timestep().as_secs_f32() * physics.relative_speed();
    for (entity, velocity, sample) in &mut bodies {
        match sample {
            Some(mut sample) => {
                let next = sample.after_step(velocity.0, dt);
                if *sample != next {
                    *sample = next;
                }
            }
            None => {
                commands.entity(entity).try_insert(BodyAcceleration {
                    last_velocity: velocity.0,
                    value: Vec2::ZERO,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_acceleration_is_the_velocity_change_over_the_step() {
        let start = BodyAcceleration::default();
        let next = start.after_step(Vec2::new(0.0, -0.5), 0.05);
        assert!((next.value - Vec2::new(0.0, -10.0)).length() < 1e-4);
        assert_eq!(next.last_velocity, Vec2::new(0.0, -0.5));
        // A paused step (dt 0) keeps the last value.
        assert_eq!(next.after_step(Vec2::ZERO, 0.0).value, next.value);
    }
}
