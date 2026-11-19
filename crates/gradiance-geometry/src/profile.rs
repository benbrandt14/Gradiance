//! A shape's projected width as a function of direction — the frontal
//! width air drag acts on (brief N-4).
//!
//! The width of a shape *across* a direction `d` is its extent along the
//! perpendicular `d⊥`: `max(p·d⊥) − min(p·d⊥)` over its outline. That is the
//! width of the convex hull, so concavities and holes do not change it. It
//! repeats every half turn (`d` and `−d` see the same width).
//!
//! [`WidthProfile`] samples that function once, at [`PROFILE_SAMPLES`]
//! directions over a half turn, from the outline [`crate::polygonize`] gives
//! (the single discretization point). A lookup is then a wrap, a lerp and no
//! allocation, so the per-step drag pass never touches the contour. The
//! sampling error is second order in the sample spacing (about 0.03 % of the
//! width for a box between samples); at a sample direction (every
//! `180° / PROFILE_SAMPLES`, so the axes) it is exact.

use crate::contours::Contours;
use bevy::math::Vec2;
use core::f32::consts::PI;

/// Directions sampled over a half turn (a power of two, so the axes and the
/// diagonals are samples).
pub const PROFILE_SAMPLES: usize = 64;

/// A shape's projected width at [`PROFILE_SAMPLES`] directions over a half
/// turn, in the shape's own frame. Fixed size: copying or reading it never
/// allocates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidthProfile {
    widths: [f32; PROFILE_SAMPLES],
}

impl WidthProfile {
    /// The zero profile: no width in any direction (an empty or infinite
    /// shape — a half-plane has no frontal width to speak of).
    pub const ZERO: Self = Self {
        widths: [0.0; PROFILE_SAMPLES],
    };

    /// Samples the width of `points` (any point set; usually an outline)
    /// across each direction. Cost `O(PROFILE_SAMPLES · n)`, run when a shape
    /// changes, never per step.
    #[must_use]
    pub fn from_points(points: &[Vec2]) -> Self {
        let mut widths = [0.0; PROFILE_SAMPLES];
        for (k, width) in widths.iter_mut().enumerate() {
            let theta = PI * k as f32 / PROFILE_SAMPLES as f32;
            // Across direction (cos θ, sin θ): extent along its normal.
            let normal = Vec2::from_angle(theta).perp();
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for p in points.iter().filter(|p| p.is_finite()) {
                let s = p.dot(normal);
                lo = lo.min(s);
                hi = hi.max(s);
            }
            *width = if hi >= lo { hi - lo } else { 0.0 };
        }
        Self { widths }
    }

    /// The profile of a contoured shape (its outline; holes lie inside it).
    #[must_use]
    pub fn from_contours(contours: &Contours) -> Self {
        Self::from_points(&contours.outline)
    }

    /// The width across `direction` (shape frame; any length). A zero
    /// direction reads as `+x`.
    #[must_use]
    pub fn width_across(&self, direction: Vec2) -> f32 {
        let angle = direction.y.atan2(direction.x).rem_euclid(PI);
        let t = angle / PI * PROFILE_SAMPLES as f32;
        let k = (t as usize).min(PROFILE_SAMPLES - 1);
        let frac = (t - k as f32).clamp(0.0, 1.0);
        let a = self.widths[k];
        let b = self.widths[(k + 1) % PROFILE_SAMPLES];
        a + (b - a) * frac
    }

    /// The largest width over every direction.
    #[must_use]
    pub fn max_width(&self) -> f32 {
        self.widths.iter().copied().fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polygonize::polygonize;
    use crate::shape::ShapeDef;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn a_plate_is_wide_face_on_and_thin_edge_on() {
        let plate = WidthProfile::from_contours(&polygonize(&ShapeDef::Box {
            width: 2.0,
            height: 0.1,
        }));
        // Falling (air moving along y): the whole 2 m width faces the flow.
        assert!(close(plate.width_across(Vec2::NEG_Y), 2.0, 1e-5));
        assert!(close(plate.width_across(Vec2::Y), 2.0, 1e-5));
        // Moving along x: only the 0.1 m edge.
        assert!(close(plate.width_across(Vec2::X), 0.1, 1e-5));
        assert!(close(plate.width_across(Vec2::NEG_X), 0.1, 1e-5));
    }

    #[test]
    fn an_oblique_direction_matches_the_exact_width_closely() {
        let (w, h) = (2.0_f32, 0.5_f32);
        let profile = WidthProfile::from_contours(&polygonize(&ShapeDef::Box {
            width: w,
            height: h,
        }));
        for deg in [7.0_f32, 33.3, 61.0, 100.0, 171.0, -20.0] {
            let a = deg.to_radians();
            // Across (cos a, sin a): extent along the normal (-sin a, cos a).
            let exact = w * a.sin().abs() + h * a.cos().abs();
            let got = profile.width_across(Vec2::from_angle(a));
            assert!(close(got, exact, exact * 0.005), "{deg}: {got} vs {exact}");
        }
    }

    #[test]
    fn a_circle_is_its_diameter_every_way() {
        let profile = WidthProfile::from_contours(&polygonize(&ShapeDef::Circle { radius: 0.5 }));
        for deg in [0.0_f32, 13.0, 45.0, 90.0, 222.0] {
            let got = profile.width_across(Vec2::from_angle(deg.to_radians()));
            assert!(close(got, 1.0, 0.01), "{deg}: {got}");
        }
        assert!(close(profile.max_width(), 1.0, 0.01));
    }

    #[test]
    fn the_zero_profile_and_no_points_have_no_width() {
        assert_eq!(WidthProfile::ZERO.width_across(Vec2::X), 0.0);
        assert_eq!(WidthProfile::from_points(&[]).width_across(Vec2::Y), 0.0);
        assert_eq!(WidthProfile::ZERO.width_across(Vec2::ZERO), 0.0);
    }
}
