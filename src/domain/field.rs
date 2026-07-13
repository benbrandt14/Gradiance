//! Field sources: authored force fields on bodies (M20, Algodoo's
//! attraction).
//!
//! A [`FieldSource`] is *not a joint* — it is a per-body field. The field
//! seam (`physics::fields`) exposes **one sampling cut-point**
//! (`Fields::accel_at`) that superposes every source; the force applicator,
//! the vector-plot overlay, "set in orbit", and future consumers (particle
//! sims, node-editor sensors, scripted probes) all read the field through
//! it. Architecture notes: `docs/field-architecture.md`.
//!
//! By default a source's field is shaped by its **SDF** — magnitude decays
//! over surface distance and direction follows the SDF gradient, so a long
//! plank repels away from its faces. (SDF-shaped is the default, not a
//! commitment; future non-body sources plug into the same sampling seam.)

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Field falloff over surface distance — Algodoo offers exactly these two.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub enum FieldFalloff {
    /// `1 / (1 + d/m)` — long reach.
    Linear,
    /// `1 / (1 + d/m)²` — gravity-like (the Algodoo default).
    #[default]
    Quadratic,
}

/// An authored field source (Algodoo's attraction/repulsion property).
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct FieldSource {
    /// Repulsion strength as acceleration at zero surface distance
    /// (px/s²), for a source of reference mass (1 m² at density 1).
    /// **Negative attracts** (the Algodoo convention: one signed repulsion
    /// knob). The field acts on *every* dynamic body, scaled by the
    /// target's mass (acceleration is mass-independent — gravity-like,
    /// orbits work), couples through the source's own mass (cutting a
    /// source conserves its field), and reacts equally and oppositely on
    /// the source (momentum conserves). See `docs/field-architecture.md`.
    pub strength: f32,
    /// Falloff over surface distance.
    pub falloff: FieldFalloff,
}

impl Default for FieldSource {
    fn default() -> Self {
        Self {
            strength: -2000.0, // attract by default: "select circles, they cluster"
            falloff: FieldFalloff::Quadratic,
        }
    }
}
