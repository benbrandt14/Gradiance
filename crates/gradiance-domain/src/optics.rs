//! Authored optics (brief S-3): a body's refractive index.
//!
//! A body with [`Optics`] lets a laser beam through, bent by Snell's law
//! (`physics::laser`, `geometry::beam::trace_optics`), and reflects it
//! inside on total internal reflection. A body without it is a mirror, as
//! every body was before S-3, so old scenes trace the same.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Glass: the index a new transparent body starts with.
pub const DEFAULT_INDEX: f32 = 1.5;

/// The smallest index a body takes (air is 1; below it a beam would bend
/// away from the normal on the way in, which no ordinary material does).
pub const MIN_INDEX: f32 = 1.0;

/// The largest index a body takes (diamond is about 2.42).
pub const MAX_INDEX: f32 = 4.0;

/// A transparent body's refractive index.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct Optics {
    /// The refractive index (1 = air, 1.33 water, 1.5 glass).
    pub index: f32,
}

impl Default for Optics {
    fn default() -> Self {
        Self {
            index: DEFAULT_INDEX,
        }
    }
}

impl Optics {
    /// The optics with the index held to `[MIN_INDEX, MAX_INDEX]` (a
    /// non-finite index reads as glass).
    #[must_use]
    pub fn sanitized(self) -> Self {
        let index = if self.index.is_finite() {
            self.index.clamp(MIN_INDEX, MAX_INDEX)
        } else {
            DEFAULT_INDEX
        };
        Self { index }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_is_held_to_its_range() {
        assert_eq!(Optics { index: 0.5 }.sanitized().index, MIN_INDEX);
        assert_eq!(Optics { index: 9.0 }.sanitized().index, MAX_INDEX);
        assert_eq!(Optics { index: f32::NAN }.sanitized().index, DEFAULT_INDEX);
        assert_eq!(Optics { index: 1.33 }.sanitized().index, 1.33);
    }
}
