//! Authored surface speed (brief S-8): a conveyor body.
//!
//! A body with [`SurfaceSpeed`] carries what rests on it along its outline,
//! as a belt does, while the body itself does not move. The physics layer
//! sets the contact tangent speed of every contact the body takes part in
//! (an avian collision hook, `physics::surface`); friction then drags the
//! other body up to that speed.

use bevy::prelude::Component;
use gradiance_units::Velocity;
use serde::{Deserialize, Serialize};

/// The fastest surface the inspector and the verb accept, in m/s.
pub const MAX_SURFACE_SPEED: f32 = 100.0;

/// The speed of a body's surface along its outline, in m/s. Positive runs
/// counter-clockwise: on the top of a body, a positive speed carries a load
/// to the left (-x), a negative speed to the right.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct SurfaceSpeed(pub Velocity);

impl SurfaceSpeed {
    /// The speed held to `±MAX_SURFACE_SPEED`, or `None` for zero or a
    /// value that is not finite (the body keeps no component).
    #[must_use]
    pub fn sanitized(self) -> Option<Self> {
        let v = self.0.0;
        (v.is_finite() && v != 0.0)
            .then(|| Self(Velocity(v.clamp(-MAX_SURFACE_SPEED, MAX_SURFACE_SPEED))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_bad_speeds_are_none_and_a_fast_one_is_held() {
        assert_eq!(SurfaceSpeed(Velocity(0.0)).sanitized(), None);
        assert_eq!(SurfaceSpeed(Velocity(f32::NAN)).sanitized(), None);
        assert_eq!(
            SurfaceSpeed(Velocity(-1e6)).sanitized(),
            Some(SurfaceSpeed(Velocity(-MAX_SURFACE_SPEED)))
        );
        assert_eq!(
            SurfaceSpeed(Velocity(1.5)).sanitized(),
            Some(SurfaceSpeed(Velocity(1.5)))
        );
    }
}
