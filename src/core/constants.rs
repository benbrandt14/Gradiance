//! Global unit and convention constants.
//!
//! These values are behavioral contracts carried over from the original
//! implementation — scenes, physics feel, and the 2.5D depth mapping all
//! depend on them. Change them only as a deliberate, versioned decision.

/// World-space pixels per simulated meter.
pub const PIXELS_PER_METER: f32 = 100.0;

/// Default gravity in world units (pixels) per second squared.
pub const GRAVITY: bevy::math::Vec2 = bevy::math::Vec2::new(0.0, -1000.0);

/// Extrusion depth contributed by each active collision-layer bit.
///
/// A body occupying layer bits `min..=max` extrudes from
/// `z = -(min * LAYER_HEIGHT)` with depth `(max - min + 1) * LAYER_HEIGHT`
/// (bit 0 is the front-most layer, bit 31 the back-most).
pub const LAYER_HEIGHT: f32 = 10.0;

/// Number of segments used when discretizing a circle into a polygon.
pub const CIRCLE_SEGMENTS: usize = 48;

/// Rendered width of the (physically infinite) ground half-plane slab.
pub const GROUND_SLAB_WIDTH: f32 = 100_000.0;

/// Rendered downward extent of the ground half-plane slab.
pub const GROUND_SLAB_DEPTH: f32 = 2_000.0;
