//! Legacy (v4) collision-layer mask — kept only so old save files can be
//! migrated, plus the shared layer-hue palette.
//!
//! The authored depth/collision truth is now
//! [`DepthBand`](crate::depth::DepthBand); nothing in the live
//! world carries a `LayerMask32` anymore.

use serde::{Deserialize, Serialize};

/// The v4 save format's 32-bit layer membership + filter mask (bit 0 was
/// the front-most render layer). Loaded only by the v4→v5 migration in
/// `persist`, which maps [`occupied_range`](Self::occupied_range) to the
/// equivalent [`DepthBand`](crate::depth::DepthBand) and drops
/// custom filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct LayerMask32 {
    /// Layers this body occupies.
    pub memberships: u32,
    /// Layers this body collides with.
    pub filters: u32,
}

impl Default for LayerMask32 {
    fn default() -> Self {
        Self {
            memberships: 1,
            filters: u32::MAX,
        }
    }
}

impl LayerMask32 {
    /// The lowest and highest occupied membership bits, if any.
    ///
    /// This is the span that drives extrusion depth: `min` fixes the
    /// front face, `max` the back.
    ///
    /// ```
    /// use gradiance_domain::layers::LayerMask32;
    ///
    /// let mask = LayerMask32 { memberships: 0b0110, filters: u32::MAX };
    /// assert_eq!(mask.occupied_range(), Some((1, 2)));
    ///
    /// let empty = LayerMask32 { memberships: 0, filters: u32::MAX };
    /// assert_eq!(empty.occupied_range(), None);
    /// ```
    pub fn occupied_range(&self) -> Option<(u32, u32)> {
        if self.memberships == 0 {
            return None;
        }
        let min = self.memberships.trailing_zeros();
        let max = self.memberships.ilog2();
        Some((min, max))
    }
}

/// A distinguishable hue (degrees) for layer `bit`, shared by the viewport
/// layer overlay and the layers UI so the same layer always shows the same
/// color. Golden-angle walk: neighboring bits get far-apart hues.
///
/// ```
/// # use gradiance_domain::layers::layer_hue;
/// assert_eq!(layer_hue(0), 0.0);
/// assert!((layer_hue(1) - 137.508).abs() < 1e-3);
/// assert!(layer_hue(7) < 360.0);
/// ```
pub fn layer_hue(bit: u32) -> f32 {
    (bit as f32 * 137.508) % 360.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occupied_range_finds_bit_extents() {
        let single = LayerMask32 {
            memberships: 0b0001,
            filters: u32::MAX,
        };
        assert_eq!(single.occupied_range(), Some((0, 0)));

        let spread = LayerMask32 {
            memberships: 0b0110,
            filters: u32::MAX,
        };
        assert_eq!(spread.occupied_range(), Some((1, 2)));

        let none = LayerMask32 {
            memberships: 0,
            filters: u32::MAX,
        };
        assert_eq!(none.occupied_range(), None);

        let top = LayerMask32 {
            memberships: 1 << 31,
            filters: u32::MAX,
        };
        assert_eq!(top.occupied_range(), Some((31, 31)));
    }
}
