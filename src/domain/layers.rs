//! Collision layers — which double as the 2.5D depth mapping.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// 32-bit collision layer membership + filter mask.
///
/// Bit 0 is the **front-most** render layer, bit 31 the back-most; a body's
/// extrusion spans exactly the layers it is a member of (depth =
/// `occupied bits × LAYER_HEIGHT`). Two bodies collide when each one's
/// `filters` intersects the other's `memberships`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
