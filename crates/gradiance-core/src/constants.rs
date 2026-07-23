//! Global unit and convention constants.
//!
//! These values are behavioral contracts carried over from the original
//! implementation — scenes, physics feel, and the 2.5D depth mapping all
//! depend on them. Change them only as a deliberate, versioned decision.

// `PIXELS_PER_METER` moved to `gradiance_units::world` — the pixel↔SI scale
// is a units concern and is confined to that one seam (tests/boundaries.rs).

/// Default gravity in metres per second squared (≈ prior 1000 px/s² at 100 px/m).
pub const GRAVITY: bevy::math::Vec2 = bevy::math::Vec2::new(0.0, -10.0);

/// The interaction plane: where the cursor picks, where gizmos/grid/snap
/// indicators draw, and where the front-most body face sits. The **single**
/// authority for plane geometry — future multi-plane 3D work parameterizes
/// this, so no other file may hard-code an overlay z.
pub const INTERACTION_PLANE_Z: f32 = 0.0;

/// Extrusion depth contributed by each active collision-layer bit.
///
/// A body occupying layer bits `min..=max` extrudes from
/// `z = -(min * LAYER_HEIGHT)` with depth `(max - min + 1) * LAYER_HEIGHT`
/// (bit 0 is the front-most layer, bit 31 the back-most).
pub const LAYER_HEIGHT: f32 = 0.1;

/// Number of segments used when discretizing a circle into a polygon.
pub const CIRCLE_SEGMENTS: usize = 48;

/// Rendered width of the (physically infinite) ground half-plane slab.
pub const GROUND_SLAB_WIDTH: f32 = 1_000.0;

/// Rendered downward extent of the ground half-plane slab.
pub const GROUND_SLAB_DEPTH: f32 = 20.0;
