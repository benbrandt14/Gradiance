//! Separating-axis overlap queries between convex polygons.
//!
//! This is the solvers' entire notion of "these two bodies are in each
//! other's way". It answers with a **minimum translation vector**: the
//! shortest push that would restore the required gap. Relaxation uses the
//! MTV directly as a correction; annealing and the shelf heuristic use only
//! the yes/no and the depth as a penalty term.
//!
//! `clearance` folds the user's requested gap into the same test, so
//! "touching" and "20 mm apart" are one code path — a pair is *violating*
//! when its true separation is below the requested clearance, and the
//! reported depth is exactly how much is missing.

use bevy::math::Vec2;

/// A violated pair: how far, and which way, to push `b` off `a`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mtv {
    /// Unit axis pointing from `a` toward `b`.
    pub axis: Vec2,
    /// Distance along `axis` that restores the requested clearance (> 0).
    pub depth: f32,
}

/// Projects `poly` onto unit axis `n`, returning `(min, max)`.
fn project(poly: &[Vec2], n: Vec2) -> (f32, f32) {
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    for p in poly {
        let d = p.dot(n);
        min = min.min(d);
        max = max.max(d);
    }
    (min, max)
}

/// Pushes each edge normal of `poly` into `axes` (unnormalized edges are
/// skipped, so a degenerate polygon simply contributes nothing).
fn edge_normals(poly: &[Vec2], axes: &mut Vec<Vec2>) {
    if poly.len() < 2 {
        return;
    }
    for i in 0..poly.len() {
        let e = poly[(i + 1) % poly.len()] - poly[i];
        let len = e.length();
        if len > 1e-9 {
            axes.push(Vec2::new(e.y, -e.x) / len);
        }
    }
}

/// The minimum translation that restores `clearance` between two convex
/// polygons, or `None` when they are already at least `clearance` apart.
///
/// Both polygons are in world space. `clearance` may be zero (touching is
/// allowed) but not negative — a negative request is clamped to zero, since
/// "allowed to interpenetrate" is not a packing constraint the callers mean.
pub fn penetration(a: &[Vec2], b: &[Vec2], clearance: f32) -> Option<Mtv> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let clearance = clearance.max(0.0);

    let mut axes: Vec<Vec2> = Vec::with_capacity(a.len() + b.len() + 1);
    edge_normals(a, &mut axes);
    edge_normals(b, &mut axes);
    if axes.is_empty() {
        // Two degenerate (point/segment-less) hulls: fall back to the axis
        // between them so coincident items still get pushed apart.
        let delta = b[0] - a[0];
        let len = delta.length();
        let axis = if len > 1e-9 { delta / len } else { Vec2::X };
        axes.push(axis);
    }

    // The MTV is the axis of *greatest* separation: pushing along it is the
    // cheapest way out. Anything at or beyond `clearance` proves the pair is
    // fine and ends the query.
    let mut best_sep = f32::MIN;
    let mut best_axis = Vec2::X;
    for n in axes {
        let (a_min, a_max) = project(a, n);
        let (b_min, b_max) = project(b, n);
        // How far apart along +n, and along -n.
        let gap_pos = b_min - a_max;
        let gap_neg = a_min - b_max;
        let (sep, dir) = if gap_pos >= gap_neg {
            (gap_pos, n)
        } else {
            (gap_neg, -n)
        };
        if sep >= clearance {
            return None;
        }
        if sep > best_sep {
            best_sep = sep;
            best_axis = dir;
        }
    }

    Some(Mtv {
        axis: best_axis,
        depth: clearance - best_sep,
    })
}

/// Whether two convex polygons violate `clearance` — [`penetration`] without
/// the axis bookkeeping (it still short-circuits on the first separating
/// axis, so this is the cheap form for accept/reject tests).
pub fn overlaps(a: &[Vec2], b: &[Vec2], clearance: f32) -> bool {
    penetration(a, b, clearance).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hull::place;
    use std::f32::consts::FRAC_PI_4;

    fn square(half: f32) -> Vec<Vec2> {
        vec![
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, half),
            Vec2::new(-half, half),
        ]
    }

    #[test]
    fn disjoint_squares_do_not_penetrate() {
        let a = place(&square(0.5), Vec2::ZERO, 0.0);
        let b = place(&square(0.5), Vec2::new(3.0, 0.0), 0.0);
        assert!(penetration(&a, &b, 0.0).is_none());
    }

    #[test]
    fn clearance_turns_a_gap_into_a_violation() {
        let a = place(&square(0.5), Vec2::ZERO, 0.0);
        // Centres 1.2 apart: faces are 0.2 apart.
        let b = place(&square(0.5), Vec2::new(1.2, 0.0), 0.0);
        assert!(penetration(&a, &b, 0.1).is_none(), "0.2 gap clears 0.1");
        let mtv = penetration(&a, &b, 0.5).expect("0.2 gap violates a 0.5 clearance");
        assert!((mtv.depth - 0.3).abs() < 1e-5, "missing exactly 0.3");
        assert!(mtv.axis.distance(Vec2::X) < 1e-5, "push b further along +x");
    }

    #[test]
    fn overlapping_squares_report_the_shallow_axis() {
        let a = place(&square(0.5), Vec2::ZERO, 0.0);
        // Deep in y, shallow in x — the MTV must take the x way out.
        let b = place(&square(0.5), Vec2::new(0.9, 0.2), 0.0);
        let mtv = penetration(&a, &b, 0.0).expect("overlapping");
        assert!((mtv.depth - 0.1).abs() < 1e-5);
        assert!(mtv.axis.distance(Vec2::X) < 1e-5);
    }

    #[test]
    fn applying_the_mtv_resolves_the_pair() {
        let a = place(&square(0.5), Vec2::ZERO, 0.0);
        for (dx, dy, rot) in [
            (0.4, 0.3, 0.0),
            (-0.2, 0.7, FRAC_PI_4),
            (0.0, 0.0, FRAC_PI_4),
            (0.95, -0.95, 0.3),
        ] {
            let center = Vec2::new(dx, dy);
            let b = place(&square(0.5), center, rot);
            let Some(mtv) = penetration(&a, &b, 0.05) else {
                continue;
            };
            // Nudge past the boundary: SAT is exact, so the resolved pair sits
            // exactly at the clearance and float noise could re-trip it.
            let moved = place(&square(0.5), center + mtv.axis * (mtv.depth + 1e-4), rot);
            assert!(
                penetration(&a, &moved, 0.05).is_none(),
                "one MTV application must clear the pair (from {center:?} rot {rot})"
            );
        }
    }

    #[test]
    fn the_mtv_axis_points_from_a_to_b() {
        let a = place(&square(0.5), Vec2::ZERO, 0.0);
        let b = place(&square(0.5), Vec2::new(-0.9, 0.0), 0.0);
        let mtv = penetration(&a, &b, 0.0).expect("overlapping");
        assert!(mtv.axis.x < 0.0, "b is to the left, so it gets pushed left");
    }

    #[test]
    fn coincident_hulls_still_produce_a_push() {
        let a = place(&square(0.5), Vec2::ZERO, 0.0);
        let b = place(&square(0.5), Vec2::ZERO, 0.0);
        let mtv = penetration(&a, &b, 0.0).expect("fully coincident");
        assert!(mtv.depth > 0.0);
        assert!(mtv.axis.is_normalized());
    }
}
