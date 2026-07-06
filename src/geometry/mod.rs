//! Pure 2D/2.5D geometry — no ECS, no engine types, fully unit-testable.
//!
//! Everything operates on plain `glam` vectors and the [`contours::Contours`]
//! polygon representation. The render and physics layers adapt these
//! results into engine types.

pub mod contour;
pub mod contours;
pub mod extrusion;
pub mod polygonize;
pub mod scale;
pub mod sdf;
pub mod snapping;
pub mod tessellate;
