//! Continuous authored depth — the V3 replacement for layer checkboxes.
//!
//! A body's depth *is* its collision volume: the authored [`DepthBand`]
//! drives both the extrusion (exact floats) and the derived contiguous
//! collision-layer bits, so **collision layer ≡ visual depth** holds by
//! construction and non-integer depths just work. There is no separate
//! filter mask: two bodies collide when their bands overlap (at layer
//! granularity), and ground half-planes collide with everything.

use gradiance_core::constants::LAYER_HEIGHT;
use gradiance_units::Length;
use serde::{Deserialize, Serialize};

/// The authored depth band of a body, in world units *into* the screen:
/// the body occupies z ∈ [−far, −near], so `near = 0` puts the front
/// face on the interaction plane.
#[derive(
    bevy::prelude::Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    bevy::reflect::Reflect,
)]
pub struct DepthBand {
    /// Depth of the front face (≥ 0; 0 = the interaction plane).
    pub near: Length,
    /// Depth of the back face (> `near`).
    pub far: Length,
}

impl Default for DepthBand {
    fn default() -> Self {
        Self::metres(0.0, LAYER_HEIGHT)
    }
}

/// Tolerance for the band → bits derivation: a band grazing a layer
/// boundary by less than this does not occupy the neighboring layer, so
/// UI snapping cannot create phantom slivers.
const EDGE_EPS: f32 = 1e-3;

impl DepthBand {
    /// Minimum band thickness (a quarter layer) — the UI snap floor, and
    /// what [`sanitized`](Self::sanitized) enforces.
    pub const MIN_THICKNESS: f32 = LAYER_HEIGHT / 4.0;

    /// The UI drag snap step (a quarter layer).
    pub const SNAP_STEP: f32 = LAYER_HEIGHT / 4.0;

    /// A band from its two face depths in metres.
    #[must_use]
    pub const fn metres(near: f32, far: f32) -> Self {
        Self {
            near: Length(near),
            far: Length(far),
        }
    }

    /// A well-formed copy: `near ≥ 0`, `far ≥ near + MIN_THICKNESS`,
    /// non-finite input replaced by the default band.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let (near, far) = (self.near.value(), self.far.value());
        if !(near.is_finite() && far.is_finite()) {
            return Self::default();
        }
        let near = near.max(0.0);
        Self::metres(near, far.max(near + Self::MIN_THICKNESS))
    }

    /// z of the front face (the extrusion's `z_front`).
    pub fn z_front(&self) -> f32 {
        -self.near.value()
    }

    /// Extrusion thickness.
    pub fn thickness(&self) -> f32 {
        (self.far - self.near).value()
    }

    /// The derived contiguous collision-layer bits: every layer
    /// (`LAYER_HEIGHT`-thick slab, bit 0 front) the band overlaps,
    /// clamped to the 32 available bits, always at least one.
    ///
    /// ```
    /// use gradiance_domain::depth::DepthBand;
    ///
    /// assert_eq!(DepthBand::metres(0.0, 0.1).bits(), 0b0001);
    /// assert_eq!(DepthBand::metres(0.05, 0.15).bits(), 0b0011);
    /// assert_eq!(DepthBand::metres(0.1, 0.2).bits(), 0b0010);
    /// ```
    pub fn bits(&self) -> u32 {
        let band = self.sanitized();
        let first = ((band.near.value() / LAYER_HEIGHT + EDGE_EPS)
            .floor()
            .max(0.0) as u32)
            .min(31);
        let last = (((band.far.value() / LAYER_HEIGHT - EDGE_EPS).ceil().max(1.0) as u32) - 1)
            .clamp(first, 31);
        let count = last - first + 1;
        (((1u64 << count) - 1) << first) as u32
    }

    /// Whether two bands overlap in actual depth (exact floats, not the
    /// quantized bits).
    pub fn overlaps(&self, other: &Self) -> bool {
        self.near < other.far && other.near < self.far
    }

    /// Both edges snapped to the quarter-layer grid, preserving at least
    /// the minimum thickness (the UI drag helper).
    #[must_use]
    pub fn snapped(self) -> Self {
        let step = Self::SNAP_STEP;
        Self::metres(
            (self.near.value() / step).round() * step,
            (self.far.value() / step).round() * step,
        )
        .sanitized()
    }

    /// The equivalent band for a legacy v4 layer bit range (`min..=max`,
    /// from `LayerMask32::occupied_range`) — the save-migration mapping.
    pub fn from_bit_range(min: u32, max: u32) -> Self {
        Self::metres(min as f32 * LAYER_HEIGHT, (max + 1) as f32 * LAYER_HEIGHT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_cover_exactly_the_overlapped_layers() {
        // Bands are metres; one layer is `LAYER_HEIGHT` (0.1 m) thick.
        // Whole layers map to their own bit.
        assert_eq!(DepthBand::metres(0.0, 0.1).bits(), 1);
        assert_eq!(DepthBand::metres(0.1, 0.2).bits(), 0b10);
        assert_eq!(DepthBand::metres(0.3, 0.6).bits(), 0b11_1000);
        // Fractional bands cover every touched layer.
        assert_eq!(DepthBand::metres(0.025, 0.075).bits(), 1);
        assert_eq!(DepthBand::metres(0.075, 0.125).bits(), 0b11);
        // Grazing a boundary does not claim the neighbor.
        assert_eq!(DepthBand::metres(0.0, 0.100_005).bits(), 1);
        assert_eq!(DepthBand::metres(0.099_995, 0.2).bits(), 0b10);
    }

    #[test]
    fn bits_clamp_to_the_32_available_layers() {
        let deep = DepthBand::metres(500.0, 900.0);
        assert_eq!(deep.bits(), 1 << 31, "beyond layer 31 collides as 31");
        let huge = DepthBand::metres(0.0, 10_000.0);
        assert_eq!(huge.bits(), u32::MAX, "spanning everything sets all bits");
    }

    #[test]
    fn sanitize_enforces_ordering_and_thickness() {
        let inverted = DepthBand::metres(20.0, 5.0).sanitized();
        assert!(inverted.far.value() >= inverted.near.value() + DepthBand::MIN_THICKNESS);
        let negative = DepthBand::metres(-3.0, 4.0).sanitized();
        assert!(negative.near.value() >= 0.0);
        let sliver = DepthBand::metres(5.0, 5.0).sanitized();
        assert!(sliver.thickness() >= DepthBand::MIN_THICKNESS);
        assert_eq!(
            DepthBand::metres(f32::NAN, 1.0).sanitized(),
            DepthBand::default()
        );
    }

    #[test]
    fn snapping_rounds_to_quarter_layers() {
        let band = DepthBand::metres(0.014, 0.137).snapped();
        assert!((band.near.value() - 0.025).abs() < 1e-6);
        assert!((band.far.value() - 0.125).abs() < 1e-6);
    }

    #[test]
    fn overlap_uses_exact_floats() {
        let a = DepthBand::metres(0.0, 10.4);
        let b = DepthBand::metres(10.6, 20.0);
        assert!(!a.overlaps(&b), "exact ranges do not overlap");
        // …though the quantized bits do (layer-granularity approximation).
        assert_ne!(a.bits() & b.bits(), 0);
    }

    #[test]
    fn v4_bit_ranges_map_to_equivalent_bands() {
        assert_eq!(DepthBand::from_bit_range(0, 0), DepthBand::metres(0.0, 0.1));
        let band = DepthBand::from_bit_range(1, 3);
        assert_eq!(band, DepthBand::metres(0.1, 0.4));
        assert_eq!(band.bits(), 0b1110, "round-trips through the bit view");
    }
}
