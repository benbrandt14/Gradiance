//! The one world-scale seam: SI ⇄ world units (pixels).
//!
//! The simulation and renderer work in **world units** (pixels); authored and
//! queried state is SI. This module is the *single definition* of the pixel
//! scale — [`PIXELS_PER_METER`]. Physics configures avian's length unit from
//! it and the renderer scales SI positions for drawing through it. Once the
//! retype pass (P2) routes those sites through the typed conversions below,
//! the raw constant references disappear and a `tests/boundaries.rs`
//! confinement check locks it in (as for `CommandStack`).

use crate::quantity::{Displacement, Length};
use bevy::math::Vec2;

/// World-space pixels per simulated metre — the fixed scale between the SI
/// authoring representation and the pixel simulation/render space.
///
/// A behavioural contract: scenes, physics feel, and the 2.5D depth mapping
/// all assume it. Change only as a deliberate, versioned decision.
pub const PIXELS_PER_METER: f32 = 100.0;

/// A length in metres → world pixels.
///
/// ```
/// use gradiance_units::{world, Length};
/// assert_eq!(world::metres_to_px(Length::metres(1.0)), 100.0);
/// ```
#[must_use]
pub fn metres_to_px(length: Length) -> f32 {
    length.value() * PIXELS_PER_METER
}

/// World pixels → a length in metres.
#[must_use]
pub fn px_to_metres(px: f32) -> Length {
    Length(px / PIXELS_PER_METER)
}

/// A planar displacement in metres → world pixels.
#[must_use]
pub fn metres_to_px_vec(displacement: Displacement) -> Vec2 {
    displacement.value() * PIXELS_PER_METER
}

/// A world-pixel position → a planar displacement in metres.
#[must_use]
pub fn px_to_metres_vec(px: Vec2) -> Displacement {
    Displacement(px / PIXELS_PER_METER)
}
