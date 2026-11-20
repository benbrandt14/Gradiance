//! A freehand stroke as one shape (brief N-6).
//!
//! The sketch tool and the `body.sketch` operation both turn a drawn
//! polyline into a [`ShapeDef`]:
//!
//! - **Open stroke:** simplified by Ramer-Douglas-Peucker ([`rdp`]) into at
//!   most [`MAX_SKETCH_SEGMENTS`] segments, each a capsule of the stroke's
//!   radius, joined as **one SDF union** (a balanced tree, so its depth stays
//!   within `MAX_CSG_DEPTH`). A stroke that simplifies to one segment is a
//!   plain capsule leaf. `polygonize` makes the collider and the mesh: no
//!   new geometry path.
//! - **Closed stroke** (it ends near where it began and encloses an area):
//!   the simplified ring fills as a [`ShapeDef::Polygon`].
//!
//! The field contour resolves a stroke only if it is a couple of contour
//! cells wide; [`resolvable_radius`] widens a stroke that is too thin for
//! the size of its drawing (see `polygonize::field_cell`).
//!
//! Pure math: no systems, no records. The command layer wraps a
//! [`Sketch`] in a body record.

use crate::chain::path_length;
use crate::contour::MAX_SAMPLES;
use crate::contours::{ring_centroid, ring_signed_area};
use crate::polygonize::field_cell;
use crate::shape::{CsgOp, MAX_CSG_DEPTH, MIN_SHAPE_SIZE, ShapeDef};
use bevy::math::Vec2;

/// The most points the sketch tool records for one stroke. Past this the
/// tool drops every other point and samples half as often, so memory stays
/// bounded however long the drag lasts.
pub const MAX_STROKE_POINTS: usize = 512;

/// The most capsule segments (leaves) one open stroke becomes. A balanced
/// union of this many placed capsules is `2 + ceil(log2 24) = 7` deep, under
/// `MAX_CSG_DEPTH`, and the field the contour samples stays cheap; past it
/// the simplification tolerance grows instead.
pub const MAX_SKETCH_SEGMENTS: usize = 24;

/// The most vertices a closed stroke's polygon keeps.
pub const MAX_SKETCH_OUTLINE: usize = 64;

/// The shortest stroke that makes a body (metres); a click is no sketch.
pub const MIN_SKETCH_LENGTH: f32 = 0.02;

/// The simplification tolerance as a fraction of the stroke's radius: a
/// dropped point lies within half the stroke's half-width of the kept line,
/// hidden under the stroke's own thickness.
pub const SIMPLIFY_FRACTION: f32 = 0.5;

/// A stroke closes when its ends are this many stroke widths apart or
/// less (or [`CLOSE_FRACTION`] of its length, whichever is larger).
pub const CLOSE_WIDTHS: f32 = 2.0;

/// A stroke closes when its ends are this fraction of its length apart or
/// less (or [`CLOSE_WIDTHS`] widths, whichever is larger).
pub const CLOSE_FRACTION: f32 = 0.08;

/// The least isoperimetric ratio (`4π·area / perimeter²`: 1 for a circle,
/// 0.785 for a square, near 0 for a stroke drawn out and back) a closed
/// stroke's ring needs to fill as a polygon.
pub const MIN_ROUNDNESS: f32 = 0.15;

/// The contour needs this many cells across a stroke to keep it whole.
pub const MIN_CELLS_ACROSS: f32 = 2.0;

/// A stroke as a body's shape: the shape in body space and the body's pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Sketch {
    /// The shape, in body space.
    pub shape: ShapeDef,
    /// The body's position (world).
    pub pos: Vec2,
    /// The body's rotation (radians).
    pub rot: f32,
    /// Whether the stroke closed and filled as a polygon.
    pub closed: bool,
    /// The stroke radius used (the asked radius, or wider when the drawing
    /// is too large to resolve it; see [`resolvable_radius`]).
    pub radius: f32,
    /// The simplified stroke, world space: the capsules' centre line, or
    /// the polygon's ring (the tool's preview draws it).
    pub line: Vec<Vec2>,
}

/// Ramer-Douglas-Peucker simplification of an open polyline: the first and
/// last points are kept, and every dropped point lies within `epsilon` of
/// the kept polyline's segment that spans it. Iterative (an explicit
/// stack), so a long stroke cannot overflow the call stack.
#[must_use]
pub fn rdp(points: &[Vec2], epsilon: f32) -> Vec<Vec2> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let mut worst = (0.0_f32, a);
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = segment_distance(*p, points[a], points[b]);
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > epsilon {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// [`rdp`] at `epsilon`, the tolerance grown by half until the result has
/// at most `max_points` points.
#[must_use]
pub fn simplify(points: &[Vec2], epsilon: f32, max_points: usize) -> Vec<Vec2> {
    let mut eps = if epsilon.is_finite() && epsilon > 0.0 {
        epsilon
    } else {
        MIN_SHAPE_SIZE
    };
    let mut out = rdp(points, eps);
    // RDP at a tolerance past the stroke's extent keeps two points, so
    // this ends; the bound is a guard rail only.
    for _ in 0..64 {
        if out.len() <= max_points.max(2) {
            break;
        }
        eps *= 1.5;
        out = rdp(points, eps);
    }
    out
}

/// Distance from `p` to the segment `a`→`b`.
fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// The stroke radius the field contour resolves for a drawing whose points
/// span `extent`: `radius` itself, or wider when the drawing is so large
/// that the contour's sample budget gives cells too coarse for it (at
/// least [`MIN_CELLS_ACROSS`] cells across the stroke).
#[must_use]
pub fn resolvable_radius(extent: Vec2, radius: f32) -> f32 {
    let mut r = radius;
    // The cell grows with the box (which grows with r), so settle it in a
    // few rounds; each widens by at most a few percent after the first.
    for _ in 0..4 {
        let size = extent + Vec2::splat(2.0 * r);
        let cell =
            field_cell(size, Some(2.0 * r)).max(size.max_element() / (MAX_SAMPLES as f32 - 8.0));
        let needed = MIN_CELLS_ACROSS * cell / 2.0;
        if r >= needed {
            break;
        }
        r = needed * 1.02;
    }
    r
}

/// The shape a stroke of `radius` drawn along `stroke` (world points, in
/// drawing order) makes, or `None` when it makes none: fewer than two
/// finite points, a radius that is not a positive length, or a stroke
/// shorter than [`MIN_SKETCH_LENGTH`].
#[must_use]
pub fn sketch_shape(stroke: &[Vec2], radius: f32) -> Option<Sketch> {
    if !(radius.is_finite() && radius >= MIN_SHAPE_SIZE) {
        return None;
    }
    if !stroke.iter().all(|p| p.is_finite()) {
        return None;
    }
    let mut points: Vec<Vec2> = Vec::with_capacity(stroke.len());
    for p in stroke {
        if points
            .last()
            .is_none_or(|last| last.distance(*p) > MIN_SHAPE_SIZE)
        {
            points.push(*p);
        }
    }
    let length = path_length(&points);
    if points.len() < 2 || !(length.is_finite() && length >= MIN_SKETCH_LENGTH) {
        return None;
    }
    if let Some(sketch) = closed_sketch(&points, radius, length) {
        return Some(sketch);
    }
    open_sketch(&points, radius)
}

/// The filled polygon of a stroke that closes, or `None` when it does not
/// (ends too far apart, too thin a loop, or a ring that crosses itself).
fn closed_sketch(points: &[Vec2], radius: f32, length: f32) -> Option<Sketch> {
    let (first, last) = (*points.first()?, *points.last()?);
    let gap = first.distance(last);
    let reach = (CLOSE_WIDTHS * 2.0 * radius).max(CLOSE_FRACTION * length);
    if gap > reach || gap > length / 4.0 {
        return None;
    }
    let mut ring = simplify(points, SIMPLIFY_FRACTION * radius, MAX_SKETCH_OUTLINE);
    // The ring closes on its own: a last point on the first is dropped.
    if ring.len() > 3 && ring[ring.len() - 1].distance(ring[0]) <= SIMPLIFY_FRACTION * radius {
        ring.pop();
    }
    if ring.len() < 3 || !ring_is_simple(&ring) {
        return None;
    }
    let area = ring_signed_area(&ring).abs();
    let perimeter = path_length(&ring) + ring[ring.len() - 1].distance(ring[0]);
    if 4.0 * std::f32::consts::PI * area < MIN_ROUNDNESS * perimeter * perimeter {
        return None;
    }
    let centroid = ring_centroid(&ring);
    let mut outline: Vec<Vec2> = ring.iter().map(|p| *p - centroid).collect();
    if ring_signed_area(&outline) < 0.0 {
        outline.reverse();
    }
    let shape = ShapeDef::Polygon {
        outline,
        holes: vec![],
    };
    shape.validate().ok()?;
    Some(Sketch {
        shape,
        pos: centroid,
        rot: 0.0,
        closed: true,
        radius,
        line: ring,
    })
}

/// The capsule union of an open stroke.
fn open_sketch(points: &[Vec2], radius: f32) -> Option<Sketch> {
    let (min, max) = points.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let radius = resolvable_radius(max - min, radius);
    let line = simplify(points, SIMPLIFY_FRACTION * radius, MAX_SKETCH_SEGMENTS + 1);
    let segments: Vec<(Vec2, Vec2)> = line
        .windows(2)
        .map(|w| (w[0], w[1]))
        .filter(|(a, b)| a.distance(*b) / 2.0 >= MIN_SHAPE_SIZE)
        .collect();
    match segments.as_slice() {
        [] => None,
        [(a, b)] => Some(Sketch {
            shape: ShapeDef::Capsule {
                half_length: a.distance(*b) / 2.0,
                radius,
            },
            pos: (*a + *b) / 2.0,
            rot: (*b - *a).to_angle(),
            closed: false,
            radius,
            line: vec![*a, *b],
        }),
        many => {
            // The body sits at the segments' length-weighted centre (the
            // stroke's centroid, overlaps aside).
            let total: f32 = many.iter().map(|(a, b)| a.distance(*b)).sum();
            let pos = many
                .iter()
                .map(|(a, b)| (*a + *b) / 2.0 * a.distance(*b))
                .sum::<Vec2>()
                / total;
            let leaves: Vec<ShapeDef> = many
                .iter()
                .map(|(a, b)| ShapeDef::Placed {
                    pos: (*a + *b) / 2.0 - pos,
                    rot: (*b - *a).to_angle(),
                    shape: Box::new(ShapeDef::Capsule {
                        half_length: a.distance(*b) / 2.0,
                        radius,
                    }),
                })
                .collect();
            let shape = balanced_union(&leaves)?;
            debug_assert!(shape.depth() <= MAX_CSG_DEPTH);
            Some(Sketch {
                shape,
                pos,
                rot: 0.0,
                closed: false,
                radius,
                line: std::iter::once(many[0].0)
                    .chain(many.iter().map(|(_, b)| *b))
                    .collect(),
            })
        }
    }
}

/// The union of `leaves` as a balanced tree (depth `ceil(log2 n)` above the
/// leaves), or `None` for no leaves.
#[must_use]
pub fn balanced_union(leaves: &[ShapeDef]) -> Option<ShapeDef> {
    match leaves {
        [] => None,
        [one] => Some(one.clone()),
        many => {
            let (lhs, rhs) = many.split_at(many.len() / 2);
            Some(ShapeDef::Csg {
                op: CsgOp::Union,
                lhs: Box::new(balanced_union(lhs)?),
                rhs: Box::new(balanced_union(rhs)?),
            })
        }
    }
}

/// The number of analytic leaves in a tree (`Placed` and `Csg` are nodes).
#[must_use]
pub fn leaf_count(shape: &ShapeDef) -> usize {
    match shape {
        ShapeDef::Csg { lhs, rhs, .. } => leaf_count(lhs) + leaf_count(rhs),
        ShapeDef::Placed { shape, .. } => leaf_count(shape),
        _ => 1,
    }
}

/// Whether the closed ring has no two non-adjacent edges that touch.
#[must_use]
pub fn ring_is_simple(ring: &[Vec2]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let edge = |i: usize| (ring[i], ring[(i + 1) % n]);
    for i in 0..n {
        for j in (i + 1)..n {
            // Adjacent edges share a vertex by construction.
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (a, b) = edge(i);
            let (c, d) = edge(j);
            if segments_touch(a, b, c, d) {
                return false;
            }
        }
    }
    true
}

/// Whether the segments `a`→`b` and `c`→`d` intersect or touch.
fn segments_touch(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let orient = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
    let on = |p: Vec2, q: Vec2, r: Vec2| {
        r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
    };
    let (d1, d2) = (orient(c, d, a), orient(c, d, b));
    let (d3, d4) = (orient(a, b, c), orient(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && on(c, d, a))
        || (d2 == 0.0 && on(c, d, b))
        || (d3 == 0.0 && on(a, b, c))
        || (d4 == 0.0 && on(a, b, d))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polygonize::polygonize_components;
    use crate::sdf;

    /// A wavy open stroke of `n` samples, `span` metres wide.
    fn wave(n: usize, span: f32, amplitude: f32) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                let t = i as f32 / (n - 1) as f32;
                Vec2::new(
                    span * t,
                    amplitude * (t * std::f32::consts::TAU * 1.5).sin(),
                )
            })
            .collect()
    }

    /// A circle stroke of `n` samples, ending `gap` radians short of its
    /// start.
    fn loop_stroke(n: usize, r: f32, gap: f32) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                let a = (std::f32::consts::TAU - gap) * i as f32 / (n - 1) as f32;
                Vec2::new(1.0 + r * a.cos(), 2.0 + r * a.sin())
            })
            .collect()
    }

    #[test]
    fn rdp_keeps_the_ends_and_drops_points_within_the_tolerance() {
        let line = [
            Vec2::ZERO,
            Vec2::new(1.0, 0.01),
            Vec2::new(2.0, -0.01),
            Vec2::new(3.0, 0.0),
        ];
        assert_eq!(rdp(&line, 0.05), vec![Vec2::ZERO, Vec2::new(3.0, 0.0)]);
        let corner = [Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)];
        assert_eq!(rdp(&corner, 0.05).len(), 3, "a corner is kept");
        assert_eq!(rdp(&corner[..2], 0.05).len(), 2);
    }

    #[test]
    fn rdp_bounds_the_deviation_of_every_dropped_point() {
        let stroke = wave(200, 4.0, 0.6);
        let eps = 0.03;
        let kept = rdp(&stroke, eps);
        assert!(kept.len() < stroke.len());
        for p in &stroke {
            let d = kept
                .windows(2)
                .map(|w| segment_distance(*p, w[0], w[1]))
                .fold(f32::MAX, f32::min);
            assert!(d <= eps + 1e-5, "{p} is {d} off");
        }
    }

    #[test]
    fn simplify_grows_the_tolerance_to_meet_the_cap() {
        let stroke = wave(500, 10.0, 2.0);
        let out = simplify(&stroke, 1e-4, 9);
        assert!(out.len() <= 9);
        assert_eq!(out.first(), stroke.first());
        assert_eq!(out.last(), stroke.last());
    }

    #[test]
    fn a_stroke_of_200_samples_is_one_union_of_fewer_than_30_capsules() {
        let stroke = wave(200, 4.0, 0.6);
        let sketch = sketch_shape(&stroke, 0.05).unwrap();
        assert!(!sketch.closed);
        let leaves = leaf_count(&sketch.shape);
        assert!((2..30).contains(&leaves), "{leaves} leaves");
        assert!(sketch.shape.depth() <= MAX_CSG_DEPTH);
        assert!(sketch.shape.validate().is_ok());
        // One body: the contour is one piece, and it covers the stroke.
        let parts = polygonize_components(&sketch.shape);
        assert_eq!(parts.len(), 1, "one component");
        for p in &stroke {
            let local = *p - sketch.pos;
            assert!(sdf::eval(&sketch.shape, local) <= 0.05 * SIMPLIFY_FRACTION + 1e-4);
        }
        // Its area is about the stroke's: length × width plus the caps.
        let length = path_length(&stroke);
        let ideal = length * 0.1;
        let area = parts[0].area();
        assert!((area - ideal).abs() / ideal < 0.2, "{area} vs {ideal}");
    }

    #[test]
    fn a_wild_scribble_still_caps_at_the_segment_limit() {
        // A zig-zag of 400 sharp corners: RDP alone would keep them all.
        let stroke: Vec<Vec2> = (0..400)
            .map(|i| Vec2::new(i as f32 * 0.01, if i % 2 == 0 { 0.0 } else { 0.3 }))
            .collect();
        let sketch = sketch_shape(&stroke, 0.03).unwrap();
        assert!(leaf_count(&sketch.shape) <= MAX_SKETCH_SEGMENTS);
        assert!(sketch.shape.depth() <= MAX_CSG_DEPTH);
    }

    #[test]
    fn a_straight_stroke_is_one_capsule_leaf_along_it() {
        let stroke = [
            Vec2::new(1.0, 1.0),
            Vec2::new(1.5, 1.5),
            Vec2::new(2.0, 2.0),
        ];
        let sketch = sketch_shape(&stroke, 0.05).unwrap();
        let ShapeDef::Capsule {
            half_length,
            radius,
        } = sketch.shape
        else {
            panic!("a capsule: {:?}", sketch.shape);
        };
        assert!((half_length - std::f32::consts::SQRT_2 / 2.0).abs() < 1e-5);
        assert!((radius - 0.05).abs() < 1e-6);
        assert!((sketch.pos - Vec2::new(1.5, 1.5)).length() < 1e-5);
        assert!((sketch.rot - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }

    #[test]
    fn a_closed_stroke_fills_as_a_ccw_polygon_around_its_centroid() {
        let stroke = loop_stroke(120, 0.5, 0.1);
        let sketch = sketch_shape(&stroke, 0.05).unwrap();
        assert!(sketch.closed);
        let ShapeDef::Polygon { outline, holes } = &sketch.shape else {
            panic!("a polygon");
        };
        assert_eq!(holes.as_slice(), &[] as &[Vec<Vec2>]);
        assert!(outline.len() >= 8 && outline.len() <= MAX_SKETCH_OUTLINE);
        assert!(ring_signed_area(outline) > 0.0, "counter-clockwise");
        assert!(ring_centroid(outline).length() < 1e-3, "centroid-relative");
        assert!((sketch.pos - Vec2::new(1.0, 2.0)).length() < 0.02);
        let area = ring_signed_area(outline);
        let ideal = std::f32::consts::PI * 0.25;
        assert!((area - ideal).abs() / ideal < 0.05, "{area}");
    }

    #[test]
    fn a_stroke_that_does_not_close_or_doubles_back_stays_a_stroke() {
        // A C: the ends a quarter turn apart.
        let c = loop_stroke(120, 0.5, std::f32::consts::FRAC_PI_2);
        assert!(!sketch_shape(&c, 0.05).unwrap().closed);
        // Out and back along a line: the ends meet but enclose nothing.
        let mut back: Vec<Vec2> = (0..50).map(|i| Vec2::new(i as f32 * 0.02, 0.0)).collect();
        back.extend((0..50).rev().map(|i| Vec2::new(i as f32 * 0.02, 0.04)));
        assert!(!sketch_shape(&back, 0.02).unwrap().closed);
        // A figure eight crosses itself: no simple ring, so a stroke.
        let eight: Vec<Vec2> = (0..=160)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / 160.0;
                Vec2::new(t.sin(), (2.0 * t).sin() / 2.0)
            })
            .collect();
        let sketch = sketch_shape(&eight, 0.03).unwrap();
        assert!(!sketch.closed);
        assert_eq!(polygonize_components(&sketch.shape).len(), 1);
    }

    #[test]
    fn a_large_drawing_widens_a_thin_stroke_until_the_contour_resolves_it() {
        // A 12 m square-ish scribble at 1 cm radius: far below a contour
        // cell at the sample budget.
        let mut stroke = wave(300, 12.0, 6.0);
        stroke.push(Vec2::new(6.0, -6.0));
        let sketch = sketch_shape(&stroke, 0.01).unwrap();
        assert!(sketch.radius > 0.01);
        assert_eq!(polygonize_components(&sketch.shape).len(), 1);
        // A small drawing keeps the asked radius.
        assert!((sketch_shape(&wave(50, 1.0, 0.2), 0.01).unwrap().radius - 0.01).abs() < 1e-6);
    }

    #[test]
    fn degenerate_strokes_make_no_shape() {
        assert!(sketch_shape(&[Vec2::ZERO], 0.05).is_none());
        assert!(sketch_shape(&[Vec2::ZERO, Vec2::new(0.001, 0.0)], 0.05).is_none());
        assert!(sketch_shape(&[Vec2::ZERO, Vec2::X], 0.0).is_none());
        assert!(sketch_shape(&[Vec2::ZERO, Vec2::X], f32::NAN).is_none());
        assert!(sketch_shape(&[Vec2::ZERO, Vec2::splat(f32::NAN)], 0.05).is_none());
        assert!(sketch_shape(&[Vec2::ZERO, Vec2::ZERO, Vec2::ZERO], 0.05).is_none());
    }

    #[test]
    fn a_balanced_union_is_shallow() {
        let leaf = ShapeDef::Circle { radius: 1.0 };
        let tree = balanced_union(&vec![leaf; 24]).unwrap();
        assert_eq!(leaf_count(&tree), 24);
        assert_eq!(tree.depth(), 6);
        assert!(balanced_union(&[]).is_none());
    }

    #[test]
    fn simple_rings_and_crossing_rings() {
        let square = [
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        assert!(ring_is_simple(&square));
        let bow = [
            Vec2::ZERO,
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ];
        assert!(!ring_is_simple(&bow));
    }
}
