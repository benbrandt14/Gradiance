//! Convex hulls and the polygon measurements the solvers optimize over.
//!
//! Every packing item is reduced to **one convex polygon** in its own body
//! frame. That is the deliberate simplification of the whole layer: convex
//! overlap has an exact, cheap, differentiable-enough answer (SAT — see
//! [`crate::sat`]), so a solver can ask "how deep are these two into each
//! other, and along which axis" thousands of times per second without
//! touching the physics engine.
//!
//! The cost is conservatism: a concave body (an `L`, a `C`, a body with a
//! bite cut out of it) packs as its hull, so nothing ever nests *into* its
//! own concavity. Results are therefore always collision-free in the real
//! scene, never tighter than the true optimum. Non-convex nesting would need
//! a convex *decomposition* per item and a no-fit-polygon inner loop; the
//! [`Solver`](crate::solver::Solver) trait is the seam where that would
//! arrive, as a new item representation rather than a new mutation path.

use bevy::math::Vec2;

/// Builds the convex hull of `points`, counter-clockwise, without the
/// duplicated closing vertex (Andrew's monotone chain).
///
/// Returns the input (deduplicated) when fewer than three points survive —
/// a degenerate "hull" a caller can still transform and measure.
pub fn convex_hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = points.iter().copied().filter(|p| p.is_finite()).collect();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.distance_squared(*b) < 1e-12);
    if pts.len() < 3 {
        return pts;
    }

    // Cross product of (o→a) × (o→b): > 0 means a counter-clockwise turn.
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);

    let mut lower: Vec<Vec2> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Vec2> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    // Both chains repeat the shared endpoints; drop them once.
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Twice the signed area of a polygon (positive when counter-clockwise).
fn double_signed_area(poly: &[Vec2]) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut acc = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        acc += a.perp_dot(b);
    }
    acc
}

/// Absolute area of a polygon.
pub fn polygon_area(poly: &[Vec2]) -> f32 {
    double_signed_area(poly).abs() * 0.5
}

/// Area centroid of a polygon, falling back to the vertex mean for
/// degenerate (zero-area) input.
pub fn polygon_centroid(poly: &[Vec2]) -> Vec2 {
    let double_area = double_signed_area(poly);
    if poly.len() < 3 || double_area.abs() < 1e-9 {
        if poly.is_empty() {
            return Vec2::ZERO;
        }
        return poly.iter().copied().sum::<Vec2>() / poly.len() as f32;
    }
    let mut acc = Vec2::ZERO;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        acc += (a + b) * a.perp_dot(b);
    }
    acc / (3.0 * double_area)
}

/// Perimeter of a closed polygon.
pub fn polygon_perimeter(poly: &[Vec2]) -> f32 {
    if poly.len() < 2 {
        return 0.0;
    }
    (0..poly.len())
        .map(|i| poly[i].distance(poly[(i + 1) % poly.len()]))
        .sum()
}

/// Largest vertex distance from the origin — the circumradius of a
/// centroid-relative hull, used as the broad-phase reject radius.
pub fn circumradius(poly: &[Vec2]) -> f32 {
    poly.iter().map(|v| v.length()).fold(0.0, f32::max)
}

/// Rotates `poly` by `rot` radians and translates it by `pos`, writing the
/// result into `out` (reused across iterations — the solvers' inner loop
/// runs this per item per step and must not allocate).
pub fn place_into(poly: &[Vec2], pos: Vec2, rot: f32, out: &mut Vec<Vec2>) {
    out.clear();
    out.reserve(poly.len());
    let (sin, cos) = rot.sin_cos();
    for v in poly {
        out.push(Vec2::new(
            pos.x + v.x * cos - v.y * sin,
            pos.y + v.x * sin + v.y * cos,
        ));
    }
}

/// Allocating form of [`place_into`].
pub fn place(poly: &[Vec2], pos: Vec2, rot: f32) -> Vec<Vec2> {
    let mut out = Vec::new();
    place_into(poly, pos, rot, &mut out);
    out
}

/// Axis-aligned bounds of a point set, or `None` when empty.
pub fn bounds(points: &[Vec2]) -> Option<(Vec2, Vec2)> {
    let mut iter = points.iter().copied();
    let first = iter.next()?;
    let mut min = first;
    let mut max = first;
    for p in iter {
        min = min.min(p);
        max = max.max(p);
    }
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn unit_square() -> Vec<Vec2> {
        vec![
            Vec2::new(-0.5, -0.5),
            Vec2::new(0.5, -0.5),
            Vec2::new(0.5, 0.5),
            Vec2::new(-0.5, 0.5),
        ]
    }

    #[test]
    fn hull_drops_interior_points_and_returns_ccw() {
        let mut pts = unit_square();
        pts.push(Vec2::ZERO);
        pts.push(Vec2::new(0.1, 0.2));
        let hull = convex_hull(&pts);
        assert_eq!(hull.len(), 4, "interior points are not on the hull");
        assert!(
            double_signed_area(&hull) > 0.0,
            "hull must come back counter-clockwise"
        );
        assert!((polygon_area(&hull) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn hull_of_collinear_points_stays_degenerate_rather_than_panicking() {
        let line: Vec<Vec2> = (0..5).map(|i| Vec2::new(i as f32, 0.0)).collect();
        let hull = convex_hull(&line);
        assert!(polygon_area(&hull) < 1e-6);
    }

    #[test]
    fn centroid_of_an_offset_square_is_its_center() {
        let poly = place(&unit_square(), Vec2::new(3.0, -2.0), 0.0);
        let c = polygon_centroid(&poly);
        assert!(c.distance(Vec2::new(3.0, -2.0)) < 1e-5);
    }

    #[test]
    fn placing_preserves_area_and_moves_the_centroid() {
        let placed = place(&unit_square(), Vec2::new(1.0, 1.0), FRAC_PI_2);
        assert!((polygon_area(&placed) - 1.0).abs() < 1e-5);
        assert!(polygon_centroid(&placed).distance(Vec2::new(1.0, 1.0)) < 1e-5);
        // A 90° turn maps the corner (-0.5,-0.5) onto (+0.5,-0.5) locally.
        let (min, max) = bounds(&placed).expect("non-empty");
        assert!((min - Vec2::new(0.5, 0.5)).length() < 1e-5);
        assert!((max - Vec2::new(1.5, 1.5)).length() < 1e-5);
    }

    #[test]
    fn circumradius_is_the_far_corner() {
        assert!((circumradius(&unit_square()) - 0.5 * 2.0_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn perimeter_of_the_unit_square_is_four() {
        assert!((polygon_perimeter(&unit_square()) - 4.0).abs() < 1e-5);
    }
}
