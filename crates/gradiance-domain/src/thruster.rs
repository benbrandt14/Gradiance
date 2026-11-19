//! Thrusters: an authored push at a point of a body (brief N-3).
//!
//! A [`Thruster`] is the payload of a placeable behavior node
//! ([`NodeKind::Thruster`](crate::node::NodeKind::Thruster)). Attached to a
//! body, it pushes that body with [`thrust`](Thruster::thrust) newtons along
//! its axis, at the node's own point: off the centre of mass the push also
//! turns the body. The axis is stored in the **body's frame**
//! ([`angle`](Thruster::angle)), so it turns with the body. A free thruster
//! (attached to nothing) has nothing to push and does nothing.
//!
//! Only the thruster is authored (saved, undone through
//! `PropertyValue::NodeKind`); the force it applies each step is derived
//! (`physics::thrust`). A signal can drive the thrust through the
//! `SignalSink::Thrust` actuator (brief N-1), which replaces the authored
//! value while it is bound and never writes it.

use bevy::math::Vec2;
use gradiance_units::{Angle, Force};
use serde::{Deserialize, Serialize};

/// The thrust a new thruster pushes with (N): the weight of a 1 m² body at
/// the default density under the default gravity, so a thruster under the
/// centre of a fresh 1 m box holds it up.
pub const DEFAULT_THRUST: f32 = 10.0;

/// An authored thruster: how hard it pushes and which way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct Thruster {
    /// The push along the axis, in newtons. Negative pushes backwards.
    pub thrust: Force,
    /// The axis the body is pushed along, as an angle from the body's local
    /// +x (radians, counter-clockwise). For a free thruster it is a world
    /// angle.
    pub angle: Angle,
}

impl Default for Thruster {
    /// Pushes [`DEFAULT_THRUST`] along the body's local +y ("up" for an
    /// unrotated body).
    fn default() -> Self {
        Self {
            thrust: Force(DEFAULT_THRUST),
            angle: Angle(core::f32::consts::FRAC_PI_2),
        }
    }
}

impl Thruster {
    /// The unit axis in the world, for a body rotated by `body_rot` radians.
    #[must_use]
    pub fn axis(&self, body_rot: f32) -> Vec2 {
        Vec2::from_angle(body_rot + self.angle.0)
    }

    /// The world force (N) of `thrust` newtons along the axis, for a body
    /// rotated by `body_rot`. `thrust` is the authored value or a signal
    /// drive's. A non-finite thrust gives no force: a NaN never reaches the
    /// solver. Pure.
    #[must_use]
    pub fn force(&self, thrust: f32, body_rot: f32) -> Vec2 {
        if !thrust.is_finite() {
            return Vec2::ZERO;
        }
        self.axis(body_rot) * thrust
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_thruster_pushes_an_unrotated_body_up() {
        let t = Thruster::default();
        let f = t.force(t.thrust.0, 0.0);
        assert!((f - Vec2::new(0.0, DEFAULT_THRUST)).length() < 1e-5, "{f}");
    }

    #[test]
    fn the_axis_turns_with_the_body() {
        let t = Thruster {
            thrust: Force(2.0),
            angle: Angle(0.0),
        };
        let f = t.force(2.0, core::f32::consts::FRAC_PI_2);
        assert!((f - Vec2::new(0.0, 2.0)).length() < 1e-5, "{f}");
        let back = t.force(-2.0, 0.0);
        assert!((back - Vec2::new(-2.0, 0.0)).length() < 1e-5, "{back}");
    }

    #[test]
    fn a_non_finite_thrust_pushes_nothing() {
        let t = Thruster::default();
        assert_eq!(t.force(f32::NAN, 0.0), Vec2::ZERO);
        assert_eq!(t.force(f32::INFINITY, 0.0), Vec2::ZERO);
    }
}
