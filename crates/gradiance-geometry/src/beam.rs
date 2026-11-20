//! A light beam that reflects off surfaces (brief N-8, the laser node).
//!
//! The beam is a polyline: it starts at an origin, runs along a direction
//! until a ray cast hits a surface, reflects there about the surface normal
//! (angle of incidence equals angle of reflection), and runs on. It ends
//! when it reaches its range, when a hit absorbs it (the laser's target), or
//! after its last allowed bounce.
//!
//! The ray cast is a parameter ([`trace_beam`]'s `cast`), so this module
//! stays pure: the physics layer casts against the engine's colliders, and
//! the tests here cast against analytic mirrors. The tracer writes into a
//! caller-owned path buffer, so a beam traced every step allocates nothing
//! once the buffer has grown to `bounces + 2` points.

use bevy::math::Vec2;

/// How far past a hit the next segment starts, along the outward normal
/// (metres). Keeps the reflected ray from hitting the surface it left.
pub const SURFACE_EPSILON: f32 = 1e-4;

/// One ray-cast hit: how far along the ray, the surface normal there (any
/// length, either side), and what was hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit<K> {
    /// Distance from the ray origin to the hit (metres).
    pub distance: f32,
    /// The surface normal at the hit. Its sign does not matter.
    pub normal: Vec2,
    /// What was hit (the physics layer passes the collider's entity).
    pub key: K,
}

/// How a traced beam ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeamEnd<K> {
    /// The path length from the origin to the end of the beam (metres).
    pub length: f32,
    /// What the last segment hit: `None` when the beam ran out of range in
    /// open space.
    pub last: Option<K>,
    /// Whether that hit absorbed the beam (`absorbs` said so).
    pub absorbed: bool,
}

/// The direction `dir` reflected about a surface with normal `normal`:
/// `d − 2 (d · n) n`. The normal's length and sign do not matter. A zero or
/// non-finite normal gives `dir` unchanged. Pure.
#[must_use]
pub fn reflect(dir: Vec2, normal: Vec2) -> Vec2 {
    let Some(n) = normal.try_normalize() else {
        return dir;
    };
    dir - 2.0 * dir.dot(n) * n
}

/// Traces a beam from `origin` along `dir` for at most `range` metres and
/// `max_bounces` reflections, writing its corners into `path` (cleared
/// first; the origin, each hit, and the end of the last segment).
///
/// `cast(origin, unit_dir, max_distance)` returns the nearest hit along a
/// ray, if any. `absorbs(key)` says whether a hit ends the beam (the
/// laser's target). A hit with a zero or non-finite normal, or a non-finite
/// distance, also ends it: there is no surface to reflect off. Pure; no
/// allocation once `path` has room for `max_bounces + 2` points.
pub fn trace_beam<K: Copy>(
    origin: Vec2,
    dir: Vec2,
    range: f32,
    max_bounces: u32,
    path: &mut Vec<Vec2>,
    mut cast: impl FnMut(Vec2, Vec2, f32) -> Option<RayHit<K>>,
    mut absorbs: impl FnMut(K) -> bool,
) -> BeamEnd<K> {
    path.clear();
    let mut end = BeamEnd {
        length: 0.0,
        last: None,
        absorbed: false,
    };
    if !origin.is_finite() {
        return end;
    }
    path.push(origin);
    let Some(mut d) = dir.try_normalize() else {
        return end;
    };
    if !range.is_finite() || range <= 0.0 {
        return end;
    }
    let mut p = origin;
    let mut remaining = range;
    let mut bounces = 0;
    loop {
        let Some(hit) = cast(p, d, remaining) else {
            path.push(p + d * remaining);
            end.length += remaining;
            end.last = None;
            return end;
        };
        if !hit.distance.is_finite() {
            return end;
        }
        let distance = hit.distance.clamp(0.0, remaining);
        let q = p + d * distance;
        path.push(q);
        end.length += distance;
        end.last = Some(hit.key);
        if absorbs(hit.key) {
            end.absorbed = true;
            return end;
        }
        let Some(n) = hit.normal.try_normalize() else {
            return end;
        };
        remaining -= distance;
        if bounces >= max_bounces || remaining <= 0.0 {
            return end;
        }
        // The normal on the side the beam came from.
        let outward = if d.dot(n) > 0.0 { -n } else { n };
        d = reflect(d, n);
        p = q + outward * SURFACE_EPSILON;
        bounces += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::{FRAC_1_SQRT_2, SQRT_2};

    /// A two-sided line mirror `{p : n · p = c}` and a disc target, cast
    /// analytically. Keys: 0 the mirror, 1 the disc.
    fn scene(
        mirrors: &[(Vec2, f32)],
        disc: Option<(Vec2, f32)>,
    ) -> impl FnMut(Vec2, Vec2, f32) -> Option<RayHit<usize>> + '_ {
        move |o, d, max| {
            let mut best: Option<RayHit<usize>> = None;
            let mut keep = |hit: RayHit<usize>| {
                if hit.distance >= 0.0
                    && hit.distance <= max
                    && best.is_none_or(|b| hit.distance < b.distance)
                {
                    best = Some(hit);
                }
            };
            for (i, (n, c)) in mirrors.iter().enumerate() {
                let denom = n.dot(d);
                if denom.abs() > 1e-9 {
                    keep(RayHit {
                        distance: (c - n.dot(o)) / denom,
                        normal: *n,
                        key: i,
                    });
                }
            }
            if let Some((centre, r)) = disc {
                let m = o - centre;
                let b = m.dot(d);
                let disc = b * b - (m.length_squared() - r * r);
                if disc >= 0.0 {
                    let t = -b - disc.sqrt();
                    keep(RayHit {
                        distance: t,
                        normal: (o + d * t - centre),
                        key: 100,
                    });
                }
            }
            best
        }
    }

    #[test]
    fn reflection_mirrors_the_normal_component() {
        let r = reflect(Vec2::new(1.0, -1.0), Vec2::new(0.0, 3.0));
        assert!((r - Vec2::new(1.0, 1.0)).length() < 1e-6, "{r}");
        // Either side of the normal gives the same reflection.
        let r2 = reflect(Vec2::new(1.0, -1.0), Vec2::new(0.0, -1.0));
        assert!((r - r2).length() < 1e-6);
        // Head-on comes straight back; a grazing ray keeps going.
        assert!((reflect(Vec2::X, Vec2::NEG_X) - Vec2::NEG_X).length() < 1e-6);
        assert!((reflect(Vec2::X, Vec2::Y) - Vec2::X).length() < 1e-6);
        // No normal, no reflection.
        assert_eq!(reflect(Vec2::X, Vec2::ZERO), Vec2::X);
        assert_eq!(reflect(Vec2::X, Vec2::NAN), Vec2::X);
    }

    #[test]
    fn a_beam_at_45_degrees_off_a_floor_mirror_reaches_the_target() {
        // From (0, 2) down at 45 degrees onto the floor y = 0: it hits at
        // (2, 0) and rises at 45 degrees through the disc at (4, 2).
        let mut cast = scene(&[(Vec2::Y, 0.0)], Some((Vec2::new(4.0, 2.0), 0.25)));
        let mut path = Vec::new();
        let end = trace_beam(
            Vec2::new(0.0, 2.0),
            Vec2::new(1.0, -1.0),
            20.0,
            4,
            &mut path,
            &mut cast,
            |k| k == 100,
        );
        assert!(end.absorbed);
        assert_eq!(end.last, Some(100));
        assert_eq!(path.len(), 3, "{path:?}");
        assert!((path[1] - Vec2::new(2.0, 0.0)).length() < 1e-4, "{path:?}");
        let expected = 2.0 * 2.0 * SQRT_2 - 0.25;
        assert!((end.length - expected).abs() < 1e-3, "{}", end.length);
        // The last point lies on the disc, on the 45-degree line.
        let entry = Vec2::new(4.0, 2.0) - Vec2::splat(FRAC_1_SQRT_2) * 0.25;
        assert!((path[2] - entry).length() < 1e-3, "{path:?}");
    }

    #[test]
    fn a_beam_that_misses_runs_to_its_range() {
        let mut path = Vec::new();
        let end = trace_beam(
            Vec2::ZERO,
            Vec2::new(0.0, 2.0),
            5.0,
            4,
            &mut path,
            |_, _, _| None::<RayHit<()>>,
            |()| true,
        );
        assert_eq!(end.last, None);
        assert!(!end.absorbed);
        assert!((end.length - 5.0).abs() < 1e-6);
        assert_eq!(path, vec![Vec2::ZERO, Vec2::new(0.0, 5.0)]);
    }

    #[test]
    fn two_facing_mirrors_stop_at_the_bounce_cap() {
        // Mirrors x = 0 and x = 1; a beam bouncing between them at a slant.
        let mut cast = scene(&[(Vec2::X, 0.0), (Vec2::X, 1.0)], None);
        let mut path = Vec::with_capacity(8);
        let end = trace_beam(
            Vec2::new(0.5, 0.0),
            Vec2::new(1.0, 0.1),
            1000.0,
            5,
            &mut path,
            &mut cast,
            |_| false,
        );
        // The origin, the first hit, and five reflected hits.
        assert_eq!(path.len(), 7, "{path:?}");
        assert!(!end.absorbed);
        assert!(end.last.is_some());
        assert!(end.length < 1000.0);
        // Each hit is on a mirror, alternating.
        for (i, p) in path.iter().enumerate().skip(1) {
            let x = if i % 2 == 1 { 1.0 } else { 0.0 };
            assert!((p.x - x).abs() < 1e-3, "{i}: {p}");
        }
        // The buffer did not grow past its first capacity.
        assert_eq!(path.capacity(), 8);
    }

    #[test]
    fn the_range_cuts_a_beam_short() {
        let mut cast = scene(&[(Vec2::X, 3.0)], None);
        let mut path = Vec::new();
        let end = trace_beam(Vec2::ZERO, Vec2::X, 2.0, 4, &mut path, &mut cast, |_| false);
        assert_eq!(end.last, None, "the mirror is out of range");
        assert!((end.length - 2.0).abs() < 1e-6);
        // In range, the remaining length runs back after the bounce.
        let end = trace_beam(Vec2::ZERO, Vec2::X, 5.0, 4, &mut path, &mut cast, |_| false);
        assert!((end.length - 5.0).abs() < 1e-3, "{}", end.length);
        assert!((path[2] - Vec2::new(1.0, 0.0)).length() < 1e-3, "{path:?}");
    }

    #[test]
    fn a_degenerate_beam_stays_at_its_origin() {
        let mut path = vec![Vec2::ONE; 3];
        let end = trace_beam(
            Vec2::ZERO,
            Vec2::ZERO,
            5.0,
            4,
            &mut path,
            |_, _, _| None::<RayHit<()>>,
            |()| false,
        );
        assert_eq!(path, vec![Vec2::ZERO]);
        assert_eq!(end.length, 0.0);
        // A hit with no normal ends the beam there.
        let end = trace_beam(
            Vec2::ZERO,
            Vec2::X,
            5.0,
            4,
            &mut path,
            |_, _, _| {
                Some(RayHit {
                    distance: 1.0,
                    normal: Vec2::ZERO,
                    key: (),
                })
            },
            |()| false,
        );
        assert_eq!(path.len(), 2);
        assert!((end.length - 1.0).abs() < 1e-6);
    }
}
