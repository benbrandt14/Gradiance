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

/// Most panels a body's drag hull keeps (brief Q-6): a hull with more
/// vertices keeps an evenly spaced subset of them (still convex).
pub const MAX_PANELS: usize = 64;

/// One edge of a body's convex hull, in the body's own frame: its
/// midpoint, outward unit normal and length. Air presses on it when its
/// normal leads into the relative air (brief Q-6, decision
/// `aero-panel-drag`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Panel {
    /// The edge's midpoint (body frame).
    pub mid: Vec2,
    /// The outward unit normal (body frame).
    pub normal: Vec2,
    /// The edge's length (m).
    pub length: f32,
}

/// A body's drag panels: the edges of its convex hull, at most
/// [`MAX_PANELS`]. Fixed size, so reading it never allocates.
///
/// For a body that does not spin, the panels facing a flow `r` present
/// `Σ L·(n̂·r̂)` = the hull's width across `r`, the same width
/// [`WidthProfile`] samples: the per-panel law reduces to the width law.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragPanels {
    panels: [Panel; MAX_PANELS],
    count: usize,
}

impl Default for DragPanels {
    fn default() -> Self {
        Self::NONE
    }
}

impl DragPanels {
    /// No panels (an empty or infinite shape).
    pub const NONE: Self = Self {
        panels: [Panel {
            mid: Vec2::ZERO,
            normal: Vec2::ZERO,
            length: 0.0,
        }; MAX_PANELS],
        count: 0,
    };

    /// The panels of the convex hull of `points` (any point set; usually
    /// every outline vertex of a shape). Cost `O(n log n)`, run when a shape
    /// changes, never per step.
    #[must_use]
    pub fn from_points(points: &[Vec2]) -> Self {
        let hull = crate::hull::convex_hull(points);
        if hull.len() < 3 {
            return Self::NONE;
        }
        // An evenly spaced subset of a convex ring is convex.
        let n = hull.len();
        let kept = n.min(MAX_PANELS);
        let ring: Vec<Vec2> = (0..kept).map(|i| hull[i * n / kept]).collect();
        let mut out = Self::NONE;
        for i in 0..kept {
            let (p, q) = (ring[i], ring[(i + 1) % kept]);
            let edge = q - p;
            let length = edge.length();
            if length <= f32::EPSILON {
                continue;
            }
            // Counter-clockwise: the outward normal is the edge turned
            // clockwise.
            out.panels[out.count] = Panel {
                mid: (p + q) * 0.5,
                normal: Vec2::new(edge.y, -edge.x) / length,
                length,
            };
            out.count += 1;
        }
        out
    }

    /// The panels.
    #[must_use]
    pub fn panels(&self) -> &[Panel] {
        &self.panels[..self.count]
    }

    /// The width the panels facing `flow` present across it (`Σ L·(n̂·r̂)`
    /// over the facing panels): the hull's width across `flow`.
    #[must_use]
    pub fn facing_width(&self, flow: Vec2) -> f32 {
        let Some(dir) = flow.try_normalize() else {
            return 0.0;
        };
        self.panels()
            .iter()
            .map(|p| p.normal.dot(dir))
            .zip(self.panels())
            .filter(|(c, _)| *c > 0.0)
            .map(|(c, p)| c * p.length)
            .sum()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_still_body_presents_its_width_through_its_panels() {
        // At the profile's sample directions both are exact.
        for shape in [
            ShapeDef::Box {
                width: 2.0,
                height: 0.3,
            },
            ShapeDef::Circle { radius: 0.5 },
        ] {
            let contours = polygonize(&shape);
            let profile = WidthProfile::from_contours(&contours);
            let panels = DragPanels::from_points(&contours.outline);
            for k in 0..PROFILE_SAMPLES {
                let d = Vec2::from_angle(PI * k as f32 / PROFILE_SAMPLES as f32);
                let (a, b) = (panels.facing_width(d), profile.width_across(d));
                assert!(
                    close(a, b, 1e-4 * (1.0 + b)),
                    "{shape:?} at {k}: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn a_long_hull_keeps_at_most_max_panels() {
        let ring: Vec<Vec2> = (0..500)
            .map(|i| Vec2::from_angle(i as f32 / 500.0 * core::f32::consts::TAU))
            .collect();
        let panels = DragPanels::from_points(&ring);
        assert_eq!(panels.panels().len(), MAX_PANELS);
        assert!(close(panels.facing_width(Vec2::Y), 2.0, 0.01));
        assert_eq!(
            DragPanels::from_points(&[Vec2::ZERO, Vec2::X]).panels(),
            &[]
        );
    }
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
