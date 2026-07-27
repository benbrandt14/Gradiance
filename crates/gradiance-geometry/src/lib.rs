//! Pure 2D/2.5D geometry — no ECS, no engine types, fully unit-testable.
//!
//! Everything operates on plain `glam` vectors and the [`contours::Contours`]
//! polygon representation. The render and physics layers adapt these
//! results into engine types.

pub mod array;
pub mod contour;
pub mod contours;
pub mod extrusion;
pub mod hull;
pub mod polygonize;
pub mod sat;
pub mod scale;
pub mod sdf;
pub mod shape;
pub mod snapping;
pub mod tessellate;

/// Wraps an angle difference into `(-π, π]` — the short way around. Used by
/// every angular servo/limit check so they agree on wrap-around.
pub fn wrap_angle(angle: f32) -> f32 {
    let wrapped = angle.rem_euclid(std::f32::consts::TAU);
    if wrapped > std::f32::consts::PI {
        wrapped - std::f32::consts::TAU
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::wrap_angle;
    use std::f32::consts::{PI, TAU};

    #[test]
    fn wrap_angle_takes_the_short_way() {
        assert!((wrap_angle(0.0)).abs() < 1e-6);
        assert!((wrap_angle(PI + 0.1) - (-PI + 0.1)).abs() < 1e-5);
        assert!((wrap_angle(-PI + 0.1) - (-PI + 0.1)).abs() < 1e-5);
        assert!((wrap_angle(3.0 * TAU + 0.25) - 0.25).abs() < 1e-4);
        assert!((wrap_angle(PI) - PI).abs() < 1e-6, "π maps to itself");
    }
}
