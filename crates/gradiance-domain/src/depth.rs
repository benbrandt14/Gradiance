//! Continuous authored depth — the V3 replacement for layer checkboxes.
//!
//! A body's depth *is* its collision volume: the authored [`DepthBand`]
//! drives both the extrusion (exact floats) and the derived contiguous
//! collision-layer bits, so **collision layer ≡ visual depth** holds by
//! construction and non-integer depths just work. There is no separate
//! filter mask: two bodies collide when their bands overlap (at layer
//! granularity), and ground half-planes collide with everything.

use gradiance_core::constants::LAYER_HEIGHT;
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
    pub near: f32,
    /// Depth of the back face (> `near`).
    pub far: f32,
}

impl Default for DepthBand {
    fn default() -> Self {
        Self {
            near: 0.0,
            far: LAYER_HEIGHT,
        }
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

    /// A well-formed copy: `near ≥ 0`, `far ≥ near + MIN_THICKNESS`,
    /// non-finite input replaced by the default band.
    #[must_use]
    pub fn sanitized(self) -> Self {
        if !(self.near.is_finite() && self.far.is_finite()) {
            return Self::default();
        }
        let near = self.near.max(0.0);
        Self {
            near,
            far: self.far.max(near + Self::MIN_THICKNESS),
        }
    }

    /// z of the front face (the extrusion's `z_front`).
    pub fn z_front(&self) -> f32 {
        -self.near
    }

    /// Extrusion thickness.
    pub fn thickness(&self) -> f32 {
        self.far - self.near
    }

    /// The derived contiguous collision-layer bits: every layer
    /// (`LAYER_HEIGHT`-thick slab, bit 0 front) the band overlaps,
    /// clamped to the 32 available bits, always at least one.
    ///
    /// ```
    /// use gradiance_domain::depth::DepthBand;
    ///
    /// assert_eq!(DepthBand { near: 0.0, far: 10.0 }.bits(), 0b0001);
    /// assert_eq!(DepthBand { near: 5.0, far: 15.0 }.bits(), 0b0011);
    /// assert_eq!(DepthBand { near: 10.0, far: 20.0 }.bits(), 0b0010);
    /// ```
    pub fn bits(&self) -> u32 {
        let band = self.sanitized();
        let first = ((band.near / LAYER_HEIGHT + EDGE_EPS).floor().max(0.0) as u32).min(31);
        let last =
            (((band.far / LAYER_HEIGHT - EDGE_EPS).ceil().max(1.0) as u32) - 1).clamp(first, 31);
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
        Self {
            near: (self.near / step).round() * step,
            far: (self.far / step).round() * step,
        }
        .sanitized()
    }

    /// The equivalent band for a legacy v4 layer bit range (`min..=max`,
    /// from `LayerMask32::occupied_range`) — the save-migration mapping.
    pub fn from_bit_range(min: u32, max: u32) -> Self {
        Self {
            near: min as f32 * LAYER_HEIGHT,
            far: (max + 1) as f32 * LAYER_HEIGHT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_cover_exactly_the_overlapped_layers() {
        // Whole layers map to their own bit.
        assert_eq!(
            DepthBand {
                near: 0.0,
                far: 10.0
            }
            .bits(),
            1
        );
        assert_eq!(
            DepthBand {
                near: 10.0,
                far: 20.0
            }
            .bits(),
            0b10
        );
        assert_eq!(
            DepthBand {
                near: 30.0,
                far: 60.0
            }
            .bits(),
            0b11_1000
        );
        // Fractional bands cover every touched layer.
        assert_eq!(
            DepthBand {
                near: 2.5,
                far: 7.5
            }
            .bits(),
            1
        );
        assert_eq!(
            DepthBand {
                near: 7.5,
                far: 12.5
            }
            .bits(),
            0b11
        );
        // Grazing a boundary does not claim the neighbor.
        assert_eq!(
            DepthBand {
                near: 0.0,
                far: 10.0005
            }
            .bits(),
            1
        );
        assert_eq!(
            DepthBand {
                near: 9.9995,
                far: 20.0
            }
            .bits(),
            0b10
        );
    }

    #[test]
    fn bits_clamp_to_the_32_available_layers() {
        let deep = DepthBand {
            near: 500.0,
            far: 900.0,
        };
        assert_eq!(deep.bits(), 1 << 31, "beyond layer 31 collides as 31");
        let huge = DepthBand {
            near: 0.0,
            far: 10_000.0,
        };
        assert_eq!(huge.bits(), u32::MAX, "spanning everything sets all bits");
    }

    #[test]
    fn sanitize_enforces_ordering_and_thickness() {
        let inverted = DepthBand {
            near: 20.0,
            far: 5.0,
        }
        .sanitized();
        assert!(inverted.far >= inverted.near + DepthBand::MIN_THICKNESS);
        let negative = DepthBand {
            near: -3.0,
            far: 4.0,
        }
        .sanitized();
        assert!(negative.near >= 0.0);
        let sliver = DepthBand {
            near: 5.0,
            far: 5.0,
        }
        .sanitized();
        assert!(sliver.thickness() >= DepthBand::MIN_THICKNESS);
        assert_eq!(
            DepthBand {
                near: f32::NAN,
                far: 1.0
            }
            .sanitized(),
            DepthBand::default()
        );
    }

    #[test]
    fn snapping_rounds_to_quarter_layers() {
        let band = DepthBand {
            near: 1.4,
            far: 13.7,
        }
        .snapped();
        assert!((band.near - 2.5).abs() < 1e-6);
        assert!((band.far - 12.5).abs() < 1e-6);
    }

    #[test]
    fn overlap_uses_exact_floats() {
        let a = DepthBand {
            near: 0.0,
            far: 10.4,
        };
        let b = DepthBand {
            near: 10.6,
            far: 20.0,
        };
        assert!(!a.overlaps(&b), "exact ranges do not overlap");
        // …though the quantized bits do (layer-granularity approximation).
        assert_ne!(a.bits() & b.bits(), 0);
    }

    #[test]
    fn v4_bit_ranges_map_to_equivalent_bands() {
        assert_eq!(
            DepthBand::from_bit_range(0, 0),
            DepthBand {
                near: 0.0,
                far: 10.0
            }
        );
        let band = DepthBand::from_bit_range(1, 3);
        assert_eq!(
            band,
            DepthBand {
                near: 10.0,
                far: 40.0
            }
        );
        assert_eq!(band.bits(), 0b1110, "round-trips through the bit view");
    }
}
