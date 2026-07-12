//! Magnetism: an authored force-field property on bodies (M20).
//!
//! A [`Magnet`] is *not a joint* — it is a per-body field source. The
//! physics seam (`physics::forces`) pairs magnet bodies each frame and
//! applies equal-and-opposite forces along the **SDF gradient** of the
//! other body's shape, so the field follows the actual surface (the "SDF
//! force fields" of the roadmap, and the substrate the P3 symbolic field
//! forces build on).

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// An authored magnetic field source. Two magnet bodies attract when the
/// product of their strengths is positive and repel when it is negative
/// (flip one body's sign to make an opposing pole).
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct Magnet {
    /// Field strength (force at one meter of surface separation, N-ish).
    /// Sign is polarity: like signs attract each other's opposites — the
    /// pairwise force uses `strength_a * strength_b`.
    pub strength: f32,
    /// Inverse-power falloff exponent over surface distance (2 = inverse
    /// square). Higher = tighter field.
    pub falloff: f32,
}

impl Default for Magnet {
    fn default() -> Self {
        Self {
            strength: 50.0,
            falloff: 2.0,
        }
    }
}
