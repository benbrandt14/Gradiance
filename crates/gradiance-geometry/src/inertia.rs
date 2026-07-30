//! Second moments of area — the 2D shape factor in a body's rotational inertia.
//!
//! The physics layer computes a body's mass and inertia **entirely in 2D** and
//! hands them to the engine explicitly, rather than letting a 3D engine derive
//! them from the collider's volume. Mass comes from
//! `gradiance_units::mass_of(density, area)` — the single density × geometry
//! seam — and the rotational half comes from here:
//!
//! ```text
//! I = density · polar_moment(contours)
//! ```
//!
//! Keeping both halves in 2D is what makes areal density (kg/m²) survive the
//! move to a 3D engine unchanged, and it is why a body's mass is identical
//! before and after that move.

use crate::contours::Contours;
use bevy::math::Vec2;

/// Second polar moment of area about the centroid, in m⁴.
///
/// This is `J = Ix + Iy`, the shape factor for rotation about the axis normal
/// to the plane. Holes are subtracted, so an annulus reports less than the
/// disc that contains it.
///
/// Follows the [`Contours`] winding convention (outline counter-clockwise,
/// holes clockwise) but corrects each ring's sign defensively, so a ring wound
/// the wrong way subtracts area rather than silently inverting the result.
/// Returns `0.0` for degenerate (zero-area or non-finite) input.
///
/// ```
/// use gradiance_geometry::contours::Contours;
/// use gradiance_geometry::inertia::polar_moment;
/// use bevy::math::Vec2;
///
/// // A 2 × 2 square: J = bh(b² + h²)/12 = 4 · 8 / 12.
/// let square = Contours {
///     outline: vec![
///         Vec2::new(-1.0, -1.0),
///         Vec2::new(1.0, -1.0),
///         Vec2::new(1.0, 1.0),
///         Vec2::new(-1.0, 1.0),
///     ],
///     holes: vec![],
/// };
/// assert!((polar_moment(&square) - 8.0 / 3.0).abs() < 1e-5);
/// ```
#[must_use]
pub fn polar_moment(contours: &Contours) -> f32 {
    // Integrate about a pivot *inside the shape*, never about the world origin.
    // The origin-based form squares the coordinates before the fan sum cancels
    // them, so a contour a few hundred metres out — which CSG reshapes routinely
    // produce, since a cut leaves the origin off-centroid — loses the whole
    // answer to f32 cancellation. Any nearby pivot fixes it; the parallel-axis
    // shift at the end is pivot-independent.
    let Some(pivot) = contours.outline.first().copied() else {
        return 0.0;
    };

    let mut area = 0.0;
    let mut first = Vec2::ZERO;
    let mut about_pivot = 0.0;
    for (index, ring) in contours.rings().enumerate() {
        let m = ring_moments(ring, pivot);
        if !m.area.is_finite() || m.area == 0.0 {
            continue;
        }
        // The outline adds, holes subtract — regardless of how each was wound.
        let want_positive = index == 0;
        let flip = if (m.area > 0.0) == want_positive {
            1.0
        } else {
            -1.0
        };
        area += m.area * flip;
        first += m.first * flip;
        about_pivot += m.polar * flip;
    }

    if !area.is_finite() || area.abs() < f32::EPSILON {
        return 0.0;
    }
    let centroid = first / area;
    let about_centroid = about_pivot - area * centroid.length_squared();
    if about_centroid.is_finite() {
        about_centroid.max(0.0)
    } else {
        0.0
    }
}

/// One ring's area, first moment, and polar second moment about `pivot`, all
/// signed by the ring's winding.
struct RingMoments {
    area: f32,
    first: Vec2,
    polar: f32,
}

fn ring_moments(ring: &[Vec2], pivot: Vec2) -> RingMoments {
    if ring.len() < 3 {
        return RingMoments {
            area: 0.0,
            first: Vec2::ZERO,
            polar: 0.0,
        };
    }
    let (mut area, mut first, mut polar) = (0.0, Vec2::ZERO, 0.0);
    for (i, raw) in ring.iter().enumerate() {
        let p = *raw - pivot;
        let q = ring[(i + 1) % ring.len()] - pivot;
        let cross = p.x * q.y - q.x * p.y;
        area += cross;
        first += (p + q) * cross;
        // ∫x² dA and ∫y² dA over the triangle fan, combined.
        polar += (p.x * p.x + p.x * q.x + q.x * q.x + p.y * p.y + p.y * q.y + q.y * q.y) * cross;
    }
    RingMoments {
        area: area / 2.0,
        first: first / 6.0,
        polar: polar / 12.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{PI, TAU};

    fn rect(width: f32, height: f32) -> Contours {
        let (w, h) = (width / 2.0, height / 2.0);
        Contours {
            outline: vec![
                Vec2::new(-w, -h),
                Vec2::new(w, -h),
                Vec2::new(w, h),
                Vec2::new(-w, h),
            ],
            holes: vec![],
        }
    }

    /// A regular `n`-gon inscribed in radius `r`, counter-clockwise.
    fn ngon(radius: f32, n: usize) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                let a = TAU * i as f32 / n as f32;
                Vec2::new(radius * a.cos(), radius * a.sin())
            })
            .collect()
    }

    #[test]
    fn rectangle_matches_the_closed_form() {
        // J = bh(b² + h²)/12
        for (b, h) in [(1.0_f32, 1.0_f32), (0.4, 2.5), (3.0, 0.2)] {
            let expected = b * h * (b * b + h * h) / 12.0;
            let got = polar_moment(&rect(b, h));
            assert!(
                (got - expected).abs() < expected * 1e-4,
                "{b}×{h}: got {got}, want {expected}"
            );
        }
    }

    #[test]
    fn disc_matches_the_closed_form() {
        // J = πr⁴/2, approached from below by an inscribed polygon.
        let r = 1.7_f32;
        let expected = PI * r.powi(4) / 2.0;
        let got = polar_moment(&Contours {
            outline: ngon(r, 512),
            holes: vec![],
        });
        assert!(
            (got - expected).abs() < expected * 1e-3,
            "got {got}, want {expected}"
        );
    }

    #[test]
    fn annulus_subtracts_the_hole() {
        // J = π(R⁴ − r⁴)/2
        let (outer, inner) = (2.0_f32, 1.0_f32);
        let expected = PI * (outer.powi(4) - inner.powi(4)) / 2.0;
        let mut hole = ngon(inner, 512);
        hole.reverse(); // clockwise, per the Contours convention
        let got = polar_moment(&Contours {
            outline: ngon(outer, 512),
            holes: vec![hole],
        });
        assert!(
            (got - expected).abs() < expected * 1e-3,
            "got {got}, want {expected}"
        );
    }

    #[test]
    fn hole_winding_is_corrected_defensively() {
        // A hole wound counter-clockwise must still subtract.
        let mut cw = ngon(1.0, 64);
        cw.reverse();
        let wound_right = Contours {
            outline: ngon(2.0, 64),
            holes: vec![cw],
        };
        let wound_wrong = Contours {
            outline: ngon(2.0, 64),
            holes: vec![ngon(1.0, 64)],
        };
        let a = polar_moment(&wound_right);
        let b = polar_moment(&wound_wrong);
        assert!((a - b).abs() < a * 1e-5, "{a} vs {b}");
    }

    #[test]
    fn is_translation_invariant() {
        // The centroid shift must cancel: the same shape far from the origin
        // has the same moment about its own centroid.
        let centred = rect(1.2, 0.7);
        let offset = Vec2::new(400.0, -250.0);
        let moved = Contours {
            outline: centred.outline.iter().map(|v| *v + offset).collect(),
            holes: vec![],
        };
        let (a, b) = (polar_moment(&centred), polar_moment(&moved));
        assert!((a - b).abs() < a * 1e-2, "{a} vs {b}");
    }

    #[test]
    fn degenerate_input_is_zero() {
        let vanishes = |outline: Vec<Vec2>| {
            polar_moment(&Contours {
                outline,
                holes: vec![],
            })
            .abs()
                < f32::EPSILON
        };
        assert!(vanishes(vec![]));
        assert!(vanishes(vec![Vec2::ZERO, Vec2::X]));
        // Zero-area (collinear) triangle.
        assert!(vanishes(vec![Vec2::ZERO, Vec2::X, Vec2::X * 2.0]));
    }
}
