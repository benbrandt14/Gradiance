//! Collision layers — which double as the 2.5D depth mapping.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// 32-bit collision layer membership + filter mask.
///
/// Layers do double duty in Gradiance: they are both the physics
/// collision filter *and* the 2.5D depth mapping. Bit 0 is the
/// **front-most** render layer, bit 31 the back-most; a body's extrusion
/// spans exactly the layers it occupies (depth = `occupied bits ×
/// LAYER_HEIGHT`).
///
/// Two bodies collide when each one's `filters` intersects the other's
/// `memberships` — the standard two-way mask test:
///
/// ```
/// use gradiance::domain::layers::LayerMask32;
///
/// // `collides` is the rule the physics seam applies.
/// let collides = |a: &LayerMask32, b: &LayerMask32| {
///     a.filters & b.memberships != 0 && b.filters & a.memberships != 0
/// };
///
/// let front = LayerMask32 { memberships: 0b0001, filters: u32::MAX };
/// let back  = LayerMask32 { memberships: 0b0010, filters: u32::MAX };
/// assert!(collides(&front, &back), "both filter everything");
///
/// // A body that only filters its own layer ignores the other.
/// let picky = LayerMask32 { memberships: 0b0001, filters: 0b0001 };
/// assert!(!collides(&picky, &back));
/// ```
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, bevy::reflect::Reflect)]
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
    /// use gradiance::domain::layers::LayerMask32;
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
