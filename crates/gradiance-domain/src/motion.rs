//! Authored initial motion (brief S-1): the velocity and spin a body starts
//! a run with.
//!
//! Velocity itself is simulation state, never saved (invariant 5). An
//! [`InitialMotion`] is the authored *intent*: saved, undone through
//! property edits, and copied onto the engine's velocities on the frame a
//! run opens from an edited layout (`command::apply_initial_motion`), never
//! when a paused run resumes.

use bevy::prelude::Component;
use gradiance_units::{AngularVelocity, Velocity2};
use serde::{Deserialize, Serialize};

/// A body's starting velocity (m/s) and spin (rad/s, counter-clockwise).
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Default,
    Serialize,
    Deserialize,
    bevy::reflect::Reflect,
)]
pub struct InitialMotion {
    /// Linear velocity of the body (m/s).
    pub velocity: Velocity2,
    /// Angular velocity (rad/s, counter-clockwise).
    pub spin: AngularVelocity,
}

impl InitialMotion {
    /// The motion with non-finite values read as 0.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let v = self.velocity.value();
        let w = self.spin.value();
        Self {
            velocity: Velocity2(if v.is_finite() {
                v
            } else {
                bevy::math::Vec2::ZERO
            }),
            spin: AngularVelocity(if w.is_finite() { w } else { 0.0 }),
        }
    }

    /// Whether the body starts at rest (the same as having no motion).
    #[must_use]
    pub fn is_rest(&self) -> bool {
        self.velocity.value() == bevy::math::Vec2::ZERO && self.spin.value() == 0.0
    }

    /// The motion seen in a mirror whose line has the world normal
    /// `mirror_normal`: the velocity reflects across the line and the spin
    /// reverses.
    #[must_use]
    pub fn mirrored(self, mirror_normal: bevy::math::Vec2) -> Self {
        let v = self.velocity.value();
        let n = mirror_normal.normalize_or_zero();
        Self {
            velocity: Velocity2(v - 2.0 * v.dot(n) * n),
            spin: AngularVelocity(-self.spin.value()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    #[test]
    fn a_mirror_reflects_the_velocity_and_reverses_the_spin() {
        let m = InitialMotion {
            velocity: Velocity2(Vec2::new(3.0, 1.0)),
            spin: AngularVelocity(2.0),
        };
        let h = m.mirrored(Vec2::X);
        assert_eq!(h.velocity.value(), Vec2::new(-3.0, 1.0));
        assert!((h.spin.value() + 2.0).abs() < 1e-6);
        assert_eq!(h.mirrored(Vec2::X), m);
        assert!(InitialMotion::default().is_rest());
        let bad = InitialMotion {
            velocity: Velocity2(Vec2::new(f32::NAN, 0.0)),
            spin: AngularVelocity(f32::INFINITY),
        };
        assert!(bad.sanitized().is_rest());
    }
}
