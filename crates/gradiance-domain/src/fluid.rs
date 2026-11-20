//! Fluid regions: a body whose shape holds a fluid (brief N-7, buoyancy).
//!
//! A [`Fluid`] marks a body as a region of fluid. Every dynamic body that
//! overlaps it is pushed up by the weight of the fluid it displaces,
//! `ρ · g · A` (Archimedes, with the submerged area `A` and the areal fluid
//! density `ρ`), at the centroid of the submerged area, and is damped in
//! proportion to that area. The forces are derived each physics step
//! (`physics::buoyancy`); this component is the authored half: saved,
//! undone through property edits, never derived.
//!
//! A fluid body is usually a static sensor (so bodies pass into it), which
//! is what the inspector's toggle and `(set-fluid …)` make it.

use bevy::prelude::Component;
use gradiance_units::Density;
use serde::{Deserialize, Serialize};

/// Default fluid density (kg/m²): twice the default body density, so a
/// default body floats half submerged.
pub const DEFAULT_FLUID_DENSITY: f32 = 2.0;

/// Default fluid drag (1/s): a floating 1 m box settles in a few bobs.
pub const DEFAULT_FLUID_DRAG: f32 = 2.0;

/// An authored fluid region on a body.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct Fluid {
    /// Areal density of the fluid (kg/m², like body density): a body
    /// lighter than this floats, a heavier one sinks.
    pub density: Density,
    /// Drag rate (1/s): the force against a submerged body's motion through
    /// the fluid is `ρ · drag · A · v`, so a body as dense as the fluid,
    /// wholly submerged, loses speed (and spin) at this rate.
    pub drag: f32,
}

impl Default for Fluid {
    fn default() -> Self {
        Self {
            density: Density(DEFAULT_FLUID_DENSITY),
            drag: DEFAULT_FLUID_DRAG,
        }
    }
}

impl Fluid {
    /// The fluid with negative or non-finite values read as 0.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let sane = |x: f32| if x.is_finite() { x.max(0.0) } else { 0.0 };
        Self {
            density: Density(sane(self.density.value())),
            drag: sane(self.drag),
        }
    }
}
