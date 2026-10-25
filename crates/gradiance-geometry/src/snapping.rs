//! Pure snap math: grid systems, feature snapping, and quantization.
//!
//! All functions take plain values so they are trivially unit-testable;
//! the interaction layer adapts settings resources into these calls.

use bevy::math::Vec2;

/// Lower edge of the preferred on-screen grid step, in logical pixels.
pub const GRID_MIN_SCREEN_STEP: f32 = 24.0;
/// Upper edge of the preferred on-screen grid step, in logical pixels.
/// The band spans a factor of four (more than two), so doubling or halving
/// from any start always lands inside it.
pub const GRID_MAX_SCREEN_STEP: f32 = 96.0;

/// Unit directions (in the unrotated grid frame) of the two line families
/// of a Cartesian grid. Vectors rather than angles so an unrotated grid is
/// exact: `from_angle(π/2)` is not exactly `(0, 1)` in f32.
pub const CARTESIAN_LINE_DIRECTIONS: [Vec2; 2] = [Vec2::X, Vec2::Y];
/// Unit directions (in the unrotated grid frame) of the two line families
/// of an isometric grid: 30° and 150°.
pub const ISOMETRIC_LINE_DIRECTIONS: [Vec2; 2] =
    [Vec2::new(0.866_025_4, 0.5), Vec2::new(-0.866_025_4, 0.5)];

/// The grid step actually in use at a given zoom: `base` scaled by a power
/// of two so that the step measures between [`GRID_MIN_SCREEN_STEP`] and
/// [`GRID_MAX_SCREEN_STEP`] logical pixels on screen.
///
/// `world_per_px` is the camera's world-units-per-logical-pixel at the
/// sandbox plane (`CameraScale`). The renderer draws with this step and the
/// snapper snaps to it, so the cursor lands on exactly the lines drawn.
/// Because the result is `base · 2ᵏ` (exact in floating point), every
/// adaptive lattice nests inside or around the authored one.
///
/// Returns `None` when `base` is not a positive, finite length: there is no
/// grid to draw or snap to (a spacing field dragged to zero must not hang
/// the ring loop or emit billions of lines). A degenerate `world_per_px`
/// (non-finite or non-positive) leaves a valid `base` unadapted.
pub fn adaptive_step(base: f32, world_per_px: f32) -> Option<f32> {
    if !(base.is_finite() && base > 0.0) {
        return None;
    }
    if !(world_per_px.is_finite() && world_per_px > 0.0) {
        return Some(base);
    }
    // f32 exponents span about ±150, so 300 halvings/doublings is a bound
    // that is never reached by real input but keeps the loop total.
    let mut step = base;
    let mut guard = 0;
    while step / world_per_px < GRID_MIN_SCREEN_STEP && guard < 300 {
        step *= 2.0;
        guard += 1;
    }
    while step / world_per_px > GRID_MAX_SCREEN_STEP && guard < 300 {
        step *= 0.5;
        guard += 1;
    }
    // Only reachable with a subnormal scale: never hand back 0 or inf.
    (step.is_finite() && step > 0.0).then_some(step)
}

/// Minor lattice steps per major step: every `GRID_MAJOR_EVERY`-th line
/// of the [`adaptive_step`] lattice (counted from the grid origin) is a
/// major line.
///
/// A power of two, so the major interval [`major_step`] is itself
/// `base · 2ʲ` — a lattice the grid draws as its *minor* step two zoom
/// doublings further out. That is what makes majors nest with zoom: a
/// line that is major at one zoom is a drawn line (major or minor) at
/// every finer zoom and at the next coarser one, and it never moves. A
/// non-power-of-two ratio (the former 5) puts majors at `5 · base · 2ᵏ`,
/// which no coarser lattice contains, so zooming out deletes them and new
/// majors appear elsewhere. Four, not eight, keeps several majors on
/// screen: minors span 24–96 px, so majors span 96–384 px.
pub const GRID_MAJOR_EVERY: i32 = 4;

/// The major-line interval for a grid drawn at `minor_step` (an
/// [`adaptive_step`] result): `minor_step · GRID_MAJOR_EVERY`, so still
/// `base · 2ʲ` and exact in floating point.
pub fn major_step(minor_step: f32) -> f32 {
    minor_step * GRID_MAJOR_EVERY as f32
}

/// Whether the line `index` adaptive steps from the grid origin (along a
/// family normal, or the `index`-th polar ring) is a major line. The
/// origin line (`index == 0`) is major too; renderers give it the axis
/// colour on top.
pub fn is_major_line(index: i32) -> bool {
    index.rem_euclid(GRID_MAJOR_EVERY) == 0
}

/// Snaps `p` to a square grid with the given origin, rotation, and spacing.
pub fn cartesian_snap(p: Vec2, origin: Vec2, rotation: f32, spacing: f32) -> Vec2 {
    snap_to_line_families(p, origin, rotation, spacing, CARTESIAN_LINE_DIRECTIONS)
}

/// Snaps `p` to an isometric grid: two families of lines along
/// [`ISOMETRIC_LINE_DIRECTIONS`], each spaced `spacing` apart (measured
/// perpendicular to the lines, as the renderer draws them). The snap
/// targets are the line intersections, a rhombic lattice whose edges are
/// `spacing / sin 120°` long.
pub fn isometric_snap(p: Vec2, origin: Vec2, rotation: f32, spacing: f32) -> Vec2 {
    snap_to_line_families(p, origin, rotation, spacing, ISOMETRIC_LINE_DIRECTIONS)
}

/// Snaps `p` to a polar grid: rings every `spacing` and
/// `angular_divisions` spokes around `origin`.
pub fn polar_snap(p: Vec2, origin: Vec2, rotation: f32, spacing: f32, divisions: u32) -> Vec2 {
    let rel = p - origin;
    let radius = rel.length();
    let snapped_radius = (radius / spacing).round() * spacing;
    if snapped_radius < spacing / 2.0 {
        return origin;
    }
    let step = std::f32::consts::TAU / divisions.max(1) as f32;
    let angle = rel.y.atan2(rel.x) - rotation;
    let snapped_angle = (angle / step).round() * step + rotation;
    origin + Vec2::from_angle(snapped_angle) * snapped_radius
}

/// Snaps to the intersections of two families of parallel lines — the same
/// description the renderer draws from: family `k` runs along
/// `directions[k]` rotated by `rotation`, and its lines sit at whole multiples of
/// `spacing` along that direction's left normal, measured from `origin`.
fn snap_to_line_families(
    p: Vec2,
    origin: Vec2,
    rotation: f32,
    spacing: f32,
    directions: [Vec2; 2],
) -> Vec2 {
    if !(spacing.is_finite() && spacing > 0.0) {
        return p;
    }
    let rot = Vec2::from_angle(rotation);
    let na = rot.rotate(directions[0]).perp();
    let nb = rot.rotate(directions[1]).perp();
    let det = na.perp_dot(nb);
    if det.abs() < f32::EPSILON {
        return p;
    }
    let rel = p - origin;
    // Nearest line of each family, then the point lying on both:
    // x·na = ca and x·nb = cb.
    let ca = (rel.dot(na) / spacing).round() * spacing;
    let cb = (rel.dot(nb) / spacing).round() * spacing;
    let x = Vec2::new(ca * nb.y - cb * na.y, na.x * cb - nb.x * ca) / det;
    origin + x
}

/// Closest point to `p` on segment `ab`.
pub fn project_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < f32::EPSILON {
        return a;
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a + ab * t
}

/// Quantizes an angle (radians) to multiples of `step` (radians).
pub fn quantize_angle(angle: f32, step: f32) -> f32 {
    if step.abs() < f32::EPSILON {
        return angle;
    }
    (angle / step).round() * step
}

/// Constrains a translation delta to an axis; `dominant` keeps only the
/// larger component (CAD shift-drag behavior).
pub fn constrain_axis(delta: Vec2, horizontal: bool, vertical: bool, dominant: bool) -> Vec2 {
    if dominant {
        if delta.x.abs() >= delta.y.abs() {
            Vec2::new(delta.x, 0.0)
        } else {
            Vec2::new(0.0, delta.y)
        }
    } else if horizontal {
        Vec2::new(delta.x, 0.0)
    } else if vertical {
        Vec2::new(0.0, delta.y)
    } else {
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cartesian_snap_rounds_to_cells_with_rotation_and_origin() {
        let p = Vec2::new(103.0, 96.0);
        assert_eq!(
            cartesian_snap(p, Vec2::ZERO, 0.0, 100.0),
            Vec2::new(100.0, 100.0)
        );

        // Origin offset shifts the lattice.
        let snapped = cartesian_snap(p, Vec2::new(50.0, 0.0), 0.0, 100.0);
        assert_eq!(snapped, Vec2::new(150.0, 100.0));

        // 45° rotated grid: the snapped point must lie on the rotated
        // lattice — its coordinates in the rotated basis are integers.
        let rot = 45_f32.to_radians();
        let snapped = cartesian_snap(Vec2::new(70.0, 2.0), Vec2::ZERO, rot, 100.0);
        let local = Vec2::from_angle(-rot).rotate(snapped) / 100.0;
        assert!((local.x - local.x.round()).abs() < 1e-4);
        assert!((local.y - local.y.round()).abs() < 1e-4);
    }

    /// Fraction-of-a-step tolerance for a point `k` steps out: a fixed
    /// floor plus a few f32 ulps of `k` (far from the origin at deep zoom,
    /// world coordinates themselves run out of resolution).
    fn tolerance(k: f32) -> f32 {
        1e-3 + k.abs() * 8.0 * f32::EPSILON
    }

    /// Asserts `q` lies on a drawn line of both families: its offset from
    /// `origin` along each family's normal is a whole number of `step`s.
    /// This is the renderer's description of the grid, not the snapper's.
    fn assert_on_drawn_lines(q: Vec2, origin: Vec2, rotation: f32, step: f32, dirs: [Vec2; 2]) {
        for dir in dirs {
            let n = Vec2::from_angle(rotation).rotate(dir).perp();
            let k = (q - origin).dot(n) / step;
            assert!(
                (k - k.round()).abs() < tolerance(k),
                "{q} is {k} steps along the {dir} family normal (step {step})"
            );
        }
    }

    #[test]
    fn isometric_snap_lands_on_drawn_line_intersections() {
        let snapped = isometric_snap(Vec2::new(90.0, 45.0), Vec2::ZERO, 0.0, 100.0);
        assert_on_drawn_lines(snapped, Vec2::ZERO, 0.0, 100.0, ISOMETRIC_LINE_DIRECTIONS);
        // Intersections along a line are spacing / sin 120° apart, so the
        // first one out along the 30° line is not at distance `spacing`.
        let along = Vec2::from_angle(30_f32.to_radians()) * 120.0;
        let first = isometric_snap(along, Vec2::ZERO, 0.0, 100.0);
        let edge = 100.0 / (120_f32.to_radians()).sin();
        assert!((first.length() - edge).abs() < 1e-2, "{first}");
    }

    #[test]
    fn adaptive_step_keeps_screen_size_in_band_and_is_a_power_of_two() {
        let mut wpp = 1e-5_f32;
        while wpp < 1e3 {
            for base in [0.001_f32, 0.25, 1.0, 100.0] {
                let step = adaptive_step(base, wpp).expect("valid base");
                let screen = step / wpp;
                assert!(
                    (GRID_MIN_SCREEN_STEP..=GRID_MAX_SCREEN_STEP).contains(&screen),
                    "screen step {screen} at base {base}, world/px {wpp}"
                );
                let k = (step / base).log2();
                assert!((k - k.round()).abs() < 1e-4, "{step} is not base·2^k");
            }
            wpp *= 1.37;
        }
    }

    #[test]
    fn adaptive_step_rejects_a_degenerate_base_and_ignores_a_degenerate_scale() {
        // No grid: a spacing dragged to (or past) zero, or garbage.
        for base in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(adaptive_step(base, 0.01), None, "base {base}");
        }
        // A broken camera scale leaves a valid base as authored.
        assert_eq!(adaptive_step(1.0, 0.0), Some(1.0));
        assert_eq!(adaptive_step(1.0, f32::NAN), Some(1.0));
    }

    /// The brief's acceptance test: across a sweep of zooms, in every grid
    /// system and with a rotated, offset origin, every snapped point lies on
    /// the lines drawn at that zoom.
    #[test]
    fn snapped_points_lie_on_the_drawn_grid_at_every_zoom() {
        let origin = Vec2::new(0.3, -0.7);
        let rotation = 0.4;
        let base = 1.0;
        let probes = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.234, 5.678),
            Vec2::new(-37.1, 12.9),
            Vec2::new(0.0071, -0.0042),
            Vec2::new(415.5, -230.25),
        ];
        let mut wpp = 1e-4_f32;
        while wpp < 10.0 {
            let step = adaptive_step(base, wpp).expect("valid base");
            assert!((GRID_MIN_SCREEN_STEP..=GRID_MAX_SCREEN_STEP).contains(&(step / wpp)));
            for p in probes {
                let q = cartesian_snap(p, origin, rotation, step);
                assert_on_drawn_lines(q, origin, rotation, step, CARTESIAN_LINE_DIRECTIONS);
                let q = isometric_snap(p, origin, rotation, step);
                assert_on_drawn_lines(q, origin, rotation, step, ISOMETRIC_LINE_DIRECTIONS);
                let q = polar_snap(p, origin, rotation, step, 12);
                let rings = (q - origin).length() / step;
                assert!(
                    (rings - rings.round()).abs() < tolerance(rings),
                    "polar {q}: {rings} rings"
                );
            }
            wpp *= 1.29;
        }
    }

    #[test]
    fn snapping_to_the_adaptive_step_differs_from_the_base_step_when_zoomed_out() {
        // 1 m base, 0.05 m/px: the base step is 20 px on screen, under the
        // band, so the drawn step is 2 m and odd metres are not grid points.
        let step = adaptive_step(1.0, 0.05).expect("valid base");
        assert!((step - 2.0).abs() < f32::EPSILON);
        let q = cartesian_snap(Vec2::new(3.1, 0.0), Vec2::ZERO, 0.0, step);
        // 4 m, the nearest drawn line; not 3 m, the nearest base line.
        assert!((q.x - 4.0).abs() < 1e-5, "{q}");
    }

    /// World offsets (from the origin, along one family normal) of the
    /// major lines drawn at `step` within `extent` — the renderer's rule:
    /// lattice index `i` at offset `i · step`, major per [`is_major_line`].
    fn majors_within(step: f32, extent: f32) -> Vec<f64> {
        let n = (extent / step).ceil() as i32;
        (-n..=n)
            .filter(|&i| is_major_line(i))
            .map(|i| f64::from(i) * f64::from(step))
            .collect()
    }

    /// The major interval is `base · 2ʲ` at every zoom, and every line the
    /// renderer classifies major sits at a whole multiple of it.
    #[test]
    fn major_lines_sit_on_a_power_of_two_multiple_of_base() {
        assert!(GRID_MAJOR_EVERY > 1);
        assert_eq!(GRID_MAJOR_EVERY.count_ones(), 1, "ratio must be 2^m");
        for base in [0.01_f32, 0.25, 1.0, 100.0] {
            let mut wpp = 1e-4_f32;
            while wpp < 1e2 {
                let minor = adaptive_step(base, wpp).expect("valid base");
                let major = major_step(minor);
                let j = (major / base).log2();
                assert!((j - j.round()).abs() < 1e-4, "{major} is not base·2^j");
                for offset in majors_within(minor, 64.0 * major) {
                    let k = offset / f64::from(major);
                    assert!(
                        (k - k.round()).abs() < 1e-9,
                        "major at {offset}, interval {major}"
                    );
                }
                wpp *= 1.37;
            }
        }
    }

    /// Majors stay put: across a sweep of zooms, every major line at one
    /// zoom is still a drawn line, at the same world position, at every
    /// finer zoom of the sweep and at the next coarser doubling — and it is
    /// still *major* at every finer zoom. Under the old 5× rule the major
    /// at `5·step` was not on the `2·step` lattice at all.
    #[test]
    fn major_lines_keep_their_world_positions_across_zoom() {
        let base = 1.0_f32;
        let mut zooms = Vec::new();
        let mut wpp = 1e-3_f32;
        while wpp < 10.0 {
            zooms.push(adaptive_step(base, wpp).expect("valid base"));
            wpp *= 1.29;
        }
        zooms.sort_by(f32::total_cmp);
        zooms.dedup();
        // A fixed window of world space, so every zoom is compared on the
        // same lines.
        let extent = 200.0 * major_step(*zooms.last().expect("non-empty sweep"));
        for (a, &coarse) in zooms.iter().enumerate() {
            for major in majors_within(coarse, extent) {
                for &fine in &zooms[..a] {
                    let i = major / f64::from(fine);
                    assert!((i - i.round()).abs() < 1e-9, "{major} not drawn at {fine}");
                    assert!(
                        is_major_line(i.round() as i32),
                        "{major} major at {coarse} but minor at {fine}"
                    );
                }
                let next = f64::from(coarse) * 2.0;
                let i = major / next;
                assert!(
                    (i - i.round()).abs() < 1e-9,
                    "{major} vanishes at the next doubling"
                );
            }
        }
    }

    /// Majors are a subset of the drawn lattice: a major offset is a whole
    /// number of minor steps, at every zoom.
    #[test]
    fn every_major_line_is_a_drawn_line() {
        let mut wpp = 1e-4_f32;
        while wpp < 10.0 {
            let minor = adaptive_step(1.0, wpp).expect("valid base");
            let k = major_step(minor) / minor;
            assert!((k - k.round()).abs() < f32::EPSILON && k >= 2.0);
            for offset in majors_within(minor, 50.0 * major_step(minor)) {
                let i = offset / f64::from(minor);
                assert!(
                    (i - i.round()).abs() < 1e-9,
                    "{offset} off the {minor} lattice"
                );
            }
            wpp *= 1.29;
        }
    }

    #[test]
    fn polar_snap_hits_ring_and_spoke() {
        let snapped = polar_snap(Vec2::new(95.0, 8.0), Vec2::ZERO, 0.0, 100.0, 8);
        assert!((snapped.length() - 100.0).abs() < 1e-3);
        assert!((snapped.y).abs() < 1e-3, "snaps onto the 0° spoke");

        // Near the origin collapses to the origin.
        assert_eq!(
            polar_snap(Vec2::new(10.0, 10.0), Vec2::ZERO, 0.0, 100.0, 8),
            Vec2::ZERO
        );
    }

    #[test]
    fn segment_projection_clamps_to_endpoints() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        assert_eq!(
            project_on_segment(Vec2::new(5.0, 3.0), a, b),
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(project_on_segment(Vec2::new(-5.0, 3.0), a, b), a);
        assert_eq!(project_on_segment(Vec2::new(15.0, 3.0), a, b), b);
    }

    #[test]
    fn angle_quantization() {
        let step = 15_f32.to_radians();
        assert!((quantize_angle(17_f32.to_radians(), step) - step).abs() < 1e-5);
        assert!((quantize_angle(-8_f32.to_radians(), step) - -step).abs() < 1e-5);
        assert!((quantize_angle(1.0, 0.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn axis_constraints() {
        let d = Vec2::new(10.0, 4.0);
        assert_eq!(constrain_axis(d, false, false, false), d);
        assert_eq!(constrain_axis(d, true, false, false), Vec2::new(10.0, 0.0));
        assert_eq!(constrain_axis(d, false, true, false), Vec2::new(0.0, 4.0));
        assert_eq!(constrain_axis(d, false, false, true), Vec2::new(10.0, 0.0));
        assert_eq!(
            constrain_axis(Vec2::new(3.0, -9.0), false, false, true),
            Vec2::new(0.0, -9.0)
        );
    }
}
