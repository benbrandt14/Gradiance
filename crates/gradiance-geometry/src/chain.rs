//! Laying a chain along a drawn path (brief N-5).
//!
//! The chain tool and the `body.chain` operation both turn a polyline into
//! the hinge points of a run of equal links. The path is split by **arc
//! length** into the whole number of links nearest to `length / link`, so
//! the chain starts and ends exactly where the path does (the end pins land
//! where the pointer pressed and released) and every link has the same arc
//! share. On a straight path each link is exactly `length / n` long; on a
//! curve a link is the chord of its arc, a little shorter.
//!
//! Pure math: no systems, no records. The command layer builds the bodies
//! and hinges from these points.

use bevy::math::Vec2;

/// The most links one chain may have. A guard rail, not a preference: a
/// long stroke at a tiny link length would otherwise ask for thousands of
/// bodies; past this the links grow longer instead.
pub const MAX_CHAIN_LINKS: usize = 200;

/// The shortest path that makes a chain (metres); a click is no chain.
pub const MIN_CHAIN_LENGTH: f32 = 0.05;

/// The total arc length of `path`.
#[must_use]
pub fn path_length(path: &[Vec2]) -> f32 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The hinge points of a chain of links about `link` long laid along `path`:
/// `n + 1` points at equal arc length from the path's first point to its
/// last, `n` the whole number of links nearest to `length / link` (at least
/// one, at most [`MAX_CHAIN_LINKS`]).
///
/// `None` when the path has fewer than two points, any coordinate is not
/// finite, `link` is not a positive finite length, or the path is shorter
/// than [`MIN_CHAIN_LENGTH`].
#[must_use]
pub fn chain_points(path: &[Vec2], link: f32) -> Option<Vec<Vec2>> {
    if path.len() < 2 || !path.iter().all(|p| p.is_finite()) {
        return None;
    }
    if !(link.is_finite() && link > 0.0) {
        return None;
    }
    let total = path_length(path);
    if !(total.is_finite() && total >= MIN_CHAIN_LENGTH) {
        return None;
    }
    // A float ratio to a small count: clamped to [1, MAX] before the cast.
    let n = ((total / link).round().clamp(1.0, MAX_CHAIN_LINKS as f32)) as usize;
    let pitch = total / n as f32;

    let mut points = Vec::with_capacity(n + 1);
    points.push(path[0]);
    // Walk the segments once; `walked` is the arc length at the start of
    // segment `seg`.
    let (mut seg, mut walked) = (0, 0.0_f32);
    for k in 1..n {
        let target = pitch * k as f32;
        while seg + 1 < path.len() - 1 && walked + path[seg].distance(path[seg + 1]) < target {
            walked += path[seg].distance(path[seg + 1]);
            seg += 1;
        }
        let (a, b) = (path[seg], path[seg + 1]);
        let span = a.distance(b);
        let t = if span > 0.0 {
            ((target - walked) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        points.push(a.lerp(b, t));
    }
    points.push(path[path.len() - 1]);
    Some(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_path_splits_into_equal_links() {
        let path = [Vec2::ZERO, Vec2::new(1.0, 0.0)];
        let points = chain_points(&path, 0.25).unwrap();
        assert_eq!(points.len(), 5, "four links");
        for (k, p) in points.iter().enumerate() {
            let x = 0.25 * k as f32;
            assert!((p.x - x).abs() < 1e-6 && p.y.abs() < 1e-6, "{p}");
        }
    }

    #[test]
    fn the_link_count_rounds_to_the_nearest_and_the_ends_are_exact() {
        // 1.1 m at 0.25 m: 4.4 links, so 4 of 0.275 m.
        let path = [Vec2::new(1.0, 2.0), Vec2::new(2.1, 2.0)];
        let points = chain_points(&path, 0.25).unwrap();
        assert_eq!(points.len(), 5);
        assert_eq!(points[0], path[0]);
        assert_eq!(points[4], path[1]);
        assert!((points[1].distance(points[0]) - 0.275).abs() < 1e-5);
    }

    #[test]
    fn a_bent_path_spaces_points_by_arc_length() {
        // An L: 1 m right, then 1 m up; eight links of 0.25 m of arc.
        let path = [Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)];
        let points = chain_points(&path, 0.25).unwrap();
        assert_eq!(points.len(), 9);
        assert!(
            (points[4] - Vec2::new(1.0, 0.0)).length() < 1e-5,
            "the corner"
        );
        assert!((points[6] - Vec2::new(1.0, 0.5)).length() < 1e-5);
        assert_eq!(points[8], Vec2::new(1.0, 1.0));
    }

    #[test]
    fn repeated_points_in_the_path_are_skipped() {
        let path = [
            Vec2::ZERO,
            Vec2::ZERO,
            Vec2::new(0.5, 0.0),
            Vec2::new(0.5, 0.0),
        ];
        let points = chain_points(&path, 0.25).unwrap();
        assert_eq!(points.len(), 3);
        assert!((points[1] - Vec2::new(0.25, 0.0)).length() < 1e-6);
        assert!(points.iter().all(|p| p.is_finite()));
    }

    #[test]
    fn a_long_path_at_a_tiny_link_is_capped() {
        let path = [Vec2::ZERO, Vec2::new(100.0, 0.0)];
        let points = chain_points(&path, 0.01).unwrap();
        assert_eq!(points.len(), MAX_CHAIN_LINKS + 1);
    }

    #[test]
    fn degenerate_input_is_no_chain() {
        assert!(chain_points(&[Vec2::ZERO], 0.25).is_none());
        assert!(chain_points(&[Vec2::ZERO, Vec2::new(0.01, 0.0)], 0.25).is_none());
        assert!(chain_points(&[Vec2::ZERO, Vec2::X], 0.0).is_none());
        assert!(chain_points(&[Vec2::ZERO, Vec2::X], f32::NAN).is_none());
        assert!(chain_points(&[Vec2::ZERO, Vec2::splat(f32::INFINITY)], 0.25).is_none());
        // A link longer than the path is one link.
        assert_eq!(chain_points(&[Vec2::ZERO, Vec2::X], 5.0).unwrap().len(), 2);
    }
}
