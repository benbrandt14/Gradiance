//! The single density-times-geometry seam.
//!
//! Everything that turns authored geometry + density into a mass goes through
//! [`mass_of`]. Today density is areal (2D, kg/m²) so mass is `density · area`;
//! a future volumetric jump (kg/m³, `density · area · thickness`) changes only
//! this function and the [`Density`](crate::quantity::Density) dimension — no
//! call site moves. See `docs/units-decision.md`.

use crate::quantity::{Area, Density, Mass};

/// The mass of a body from its areal density and cross-sectional area.
///
/// ```
/// use gradiance_units::{mass_of, Area, Density};
/// // 3 kg/m² over 2 m² is 6 kg.
/// let m = mass_of(Density::kg_per_square_metre(3.0), Area::square_metres(2.0));
/// assert_eq!(m.value(), 6.0);
/// ```
#[must_use]
pub fn mass_of(density: Density, area: Area) -> Mass {
    density * area
}
