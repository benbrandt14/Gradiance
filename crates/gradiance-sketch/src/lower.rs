//! Turn a settled sketch into authored body geometry.
//!
//! This is the **only** 2D-specific module in the crate. [`crate::doc`] and
//! [`crate::solve`](mod@crate::solve) speak SolveSpace's native 3D workplane; lowering is where a
//! sketch becomes a flat [`ShapeDef`]. A future 3D backend adds a sibling here
//! rather than changing anything upstream of it.
//!
//! Lowering introduces no new shape variant, so every derived consumer —
//! colliders, meshes, snapping — keeps working through the existing single
//! discretization point.
//!
//! # Analytic recognition
//!
//! A sketch that *is* a circle lowers to [`ShapeDef::Circle`], and an
//! axis-aligned four-sided loop lowers to [`ShapeDef::Box`]; everything else
//! becomes a [`ShapeDef::Polygon`]. This is not an optimisation, it is
//! correctness: polygonising every circle would make it a 48-gon — no longer an
//! exact SDF, visibly faceted under zoom, and different under CSG.
//!
//! Recognition reads the *solved geometry*, never which tool drew it. So a
//! polygon dragged square becomes a `Box`, and a `Box` pulled off-axis degrades
//! to a `Polygon` on its own. That is what makes "box tool" honestly a shortcut
//! for a sketch that happens to be a rectangle, rather than a separate kind of
//! object.

use bevy::math::Vec2;
use gradiance_core::constants::CIRCLE_SEGMENTS;
use gradiance_geometry::contours::{Contours, ring_signed_area};
use gradiance_geometry::shape::ShapeDef;
use std::collections::HashMap;
use thiserror::Error;

use crate::doc::{CUBIC_SEGMENTS, SketchDoc, SketchEntity, SketchId, cubic_at};

/// Why a sketch could not be turned into body geometry.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LowerError {
    /// The sketch has no profile geometry (it may be entirely construction).
    #[error("sketch has no profile geometry to lower")]
    Empty,
    /// A chain of segments does not close into a loop.
    ///
    /// Carries a point where the profile is open — the natural thing to
    /// highlight in the editor.
    #[error("profile is not closed at point {0:?}")]
    OpenProfile(SketchId),
    /// An entity referenced a point that is not in the document.
    #[error("profile references unknown point {0:?}")]
    UnknownPoint(SketchId),
    /// Every loop enclosed zero area.
    #[error("profile encloses no area")]
    Degenerate,
}

/// Lower a solved sketch to centroid-relative body geometry.
///
/// Construction geometry is excluded. Closed loops become rings: the
/// largest-area loop is the outline, the rest are holes. Winding is normalised
/// to the engine convention (outline counter-clockwise, holes clockwise), and
/// the result is translated so the centroid sits at the body origin.
///
/// # Errors
///
/// Returns [`LowerError`] if the profile is empty, open, degenerate, or refers
/// to missing points.
pub fn to_contours(doc: &SketchDoc) -> Result<Contours, LowerError> {
    let mut contours = raw_contours(doc)?;
    // Body-local space is centroid-relative.
    let c = contours.centroid();
    for v in contours
        .outline
        .iter_mut()
        .chain(contours.holes.iter_mut().flatten())
    {
        *v -= c;
    }
    Ok(contours)
}

/// Trace the profile into wound rings, still in sketch space.
fn raw_contours(doc: &SketchDoc) -> Result<Contours, LowerError> {
    let pos: HashMap<SketchId, Vec2> = doc.points.iter().map(|p| (p.id, p.at)).collect();

    let profile: Vec<&SketchEntity> = doc
        .entities
        .iter()
        .filter(|e| !doc.is_construction(e.id()))
        .collect();
    if profile.is_empty() {
        return Err(LowerError::Empty);
    }

    let mut rings: Vec<Vec<Vec2>> = Vec::new();

    // Circles are self-contained loops; everything else has to be chained.
    let mut chained: Vec<&SketchEntity> = Vec::new();
    for e in profile {
        match *e {
            SketchEntity::Circle { center, radius, .. } => {
                let c = *pos.get(&center).ok_or(LowerError::UnknownPoint(center))?;
                rings.push(sample_circle(c, radius));
            }
            _ => chained.push(e),
        }
    }

    if !chained.is_empty() {
        rings.extend(trace_loops(&chained, &pos)?);
    }

    let mut rings: Vec<(f32, Vec<Vec2>)> = rings
        .into_iter()
        .map(|r| (ring_signed_area(&r).abs(), r))
        .filter(|(a, _)| *a > f32::EPSILON)
        .collect();
    if rings.is_empty() {
        return Err(LowerError::Degenerate);
    }

    // Largest enclosed area is the outline; the rest are holes.
    rings.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut iter = rings.into_iter().map(|(_, r)| r);
    let Some(outline) = iter.next() else {
        return Err(LowerError::Degenerate);
    };

    Ok(Contours {
        outline: wind(outline, true),
        holes: iter.map(|r| wind(r, false)).collect(),
    })
}

/// Lower a solved sketch straight to a [`ShapeDef`].
///
/// # Errors
///
/// As [`to_contours`].
pub fn to_shape(doc: &SketchDoc) -> Result<ShapeDef, LowerError> {
    Ok(to_shape_with_origin(doc)?.0)
}

/// Lower a solved sketch, also reporting where the body belongs.
///
/// [`to_contours`] returns geometry in body-local (centroid-relative) space,
/// which on its own loses track of where in the world the sketch was drawn.
/// This returns both halves: the shape, and the sketch-space centroid that
/// becomes the body's pose. A caller that spawns a body needs both.
///
/// # Errors
///
/// As [`to_contours`].
pub fn to_shape_with_origin(doc: &SketchDoc) -> Result<(ShapeDef, Vec2), LowerError> {
    // A lone circle is taken at its word: the centre is exact, so the body's
    // pose comes from the document rather than from a polygon's centroid.
    if let Some((radius, centre)) = as_circle(doc) {
        return Ok((ShapeDef::Circle { radius }, centre));
    }

    let raw = raw_contours(doc)?;
    let origin = raw.centroid();
    let c = to_contours(doc)?;

    if c.holes.is_empty()
        && let Some((width, height)) = as_box(&c.outline)
    {
        return Ok((ShapeDef::Box { width, height }, origin));
    }

    Ok((
        ShapeDef::Polygon {
            outline: c.outline,
            holes: c.holes,
        },
        origin,
    ))
}

/// How far off-axis a solved edge may sit and still count as axis-aligned,
/// relative to the profile's own size.
///
/// The solver settles constraints to roughly 1e-6, so this is loose enough to
/// accept a genuinely constrained rectangle and far tighter than the 5-degree
/// tolerance the line tool uses to *infer* an axis constraint in the first
/// place. A rectangle that reaches here without those constraints — dragged
/// square by hand — is only recognised while it stays square, which is the
/// honest answer.
const AXIS_EPS: f32 = 1e-4;

/// The profile radius and centre when the sketch is exactly one circle.
fn as_circle(doc: &SketchDoc) -> Option<(f32, Vec2)> {
    let mut profile = doc.entities.iter().filter(|e| !doc.is_construction(e.id()));
    let first = profile.next()?;
    if profile.next().is_some() {
        return None;
    }
    let SketchEntity::Circle { center, radius, .. } = *first else {
        return None;
    };
    // A non-positive radius is a degenerate sketch, not a circle; falling
    // through lets `raw_contours` report it as such.
    if radius <= f32::EPSILON {
        return None;
    }
    Some((radius, doc.point(center)?.at))
}

/// The width and height when `outline` is an axis-aligned rectangle.
fn as_box(outline: &[Vec2]) -> Option<(f32, f32)> {
    let [a, b, c, d] = outline else { return None };
    let (min, max) = outline.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let (width, height) = (max.x - min.x, max.y - min.y);
    if width <= f32::EPSILON || height <= f32::EPSILON {
        return None;
    }

    // Every edge must run along one axis. Four axis-aligned edges forming a
    // closed ring can only be a rectangle, so this is the whole test — opposite
    // sides being equal follows rather than needing its own check.
    let tol = AXIS_EPS * width.max(height);
    let axis_aligned = [(a, b), (b, c), (c, d), (d, a)].into_iter().all(|(p, q)| {
        let e = *q - *p;
        e.x.abs() <= tol || e.y.abs() <= tol
    });

    axis_aligned.then_some((width, height))
}

/// Force a ring's winding: counter-clockwise when `ccw`, clockwise otherwise.
fn wind(mut ring: Vec<Vec2>, ccw: bool) -> Vec<Vec2> {
    if (ring_signed_area(&ring) > 0.0) != ccw {
        ring.reverse();
    }
    ring
}

/// Walk line/arc segments into closed loops.
///
/// Each segment is visited exactly once. A vertex whose degree is not two makes
/// the profile open, which is reported rather than silently repaired.
fn trace_loops(
    segments: &[&SketchEntity],
    pos: &HashMap<SketchId, Vec2>,
) -> Result<Vec<Vec<Vec2>>, LowerError> {
    // point -> (segment index, the point at the far end)
    let mut adj: HashMap<SketchId, Vec<(usize, SketchId)>> = HashMap::new();
    for (i, e) in segments.iter().enumerate() {
        let (a, b) = endpoints(e);
        adj.entry(a).or_default().push((i, b));
        adj.entry(b).or_default().push((i, a));
    }
    if let Some((id, _)) = adj.iter().find(|(_, links)| links.len() != 2) {
        return Err(LowerError::OpenProfile(*id));
    }

    let mut used = vec![false; segments.len()];
    let mut loops = Vec::new();

    for start_seg in 0..segments.len() {
        if used[start_seg] {
            continue;
        }
        let (start_point, _) = endpoints(segments[start_seg]);
        let mut ring: Vec<Vec2> = Vec::new();
        let mut at = start_point;
        let mut seg = start_seg;

        loop {
            used[seg] = true;
            let (a, b) = endpoints(segments[seg]);
            let to = if at == a { b } else { a };
            ring.extend(sample(segments[seg], at, to, pos)?);
            at = to;

            let Some(links) = adj.get(&at) else {
                return Err(LowerError::OpenProfile(at));
            };
            let Some(&(next, _)) = links.iter().find(|(i, _)| !used[*i]) else {
                break;
            };
            seg = next;
        }

        if at != start_point {
            return Err(LowerError::OpenProfile(at));
        }
        if ring.len() >= 3 {
            loops.push(ring);
        }
    }
    Ok(loops)
}

/// The two points a chained segment connects.
fn endpoints(e: &SketchEntity) -> (SketchId, SketchId) {
    match *e {
        SketchEntity::Line { a, b, .. } => (a, b),
        // An arc and a bezier are both "a curve between two endpoints" as far
        // as loop tracing is concerned; only sampling differs.
        SketchEntity::Arc { start, end, .. } | SketchEntity::Cubic { start, end, .. } => {
            (start, end)
        }
        // Circles are never chained; they are emitted as whole rings.
        SketchEntity::Circle { center, .. } => (center, center),
    }
}

/// Sample one segment travelling `from` -> `to`, excluding the final point
/// (the next segment in the loop contributes it).
fn sample(
    e: &SketchEntity,
    from: SketchId,
    to: SketchId,
    pos: &HashMap<SketchId, Vec2>,
) -> Result<Vec<Vec2>, LowerError> {
    let at = |id: SketchId| pos.get(&id).copied().ok_or(LowerError::UnknownPoint(id));
    match *e {
        SketchEntity::Line { .. } => Ok(vec![at(from)?]),
        SketchEntity::Arc { center, start, .. } => {
            let c = at(center)?;
            let p0 = at(start)?;
            let (a, b) = (at(from)?, at(to)?);
            let radius = (p0 - c).length();
            // The document stores arcs counter-clockwise from `start`; a walk
            // may traverse them either way.
            let mut pts = sample_arc(c, radius, a - c, b - c);
            if from != start {
                pts.reverse();
            }
            pts.pop();
            Ok(pts)
        }
        SketchEntity::Cubic {
            start,
            start_control,
            end_control,
            end,
            ..
        } => {
            let (p0, c0, c1, p1) = (at(start)?, at(start_control)?, at(end_control)?, at(end)?);
            let mut pts: Vec<Vec2> = (0..=CUBIC_SEGMENTS)
                .map(|i| {
                    let t = (i as f32) / (CUBIC_SEGMENTS as f32);
                    cubic_at(p0, c0, c1, p1, t)
                })
                .collect();
            if from != start {
                pts.reverse();
            }
            pts.pop();
            Ok(pts)
        }
        SketchEntity::Circle { .. } => Ok(Vec::new()),
    }
}

/// Points along the counter-clockwise arc from `v0` to `v1` (both relative to
/// `center`), inclusive of both ends.
fn sample_arc(center: Vec2, radius: f32, v0: Vec2, v1: Vec2) -> Vec<Vec2> {
    let a0 = v0.y.atan2(v0.x);
    let a1 = v1.y.atan2(v1.x);
    let mut sweep = a1 - a0;
    while sweep <= 0.0 {
        sweep += std::f32::consts::TAU;
    }
    let steps = arc_steps(sweep);
    (0..=steps)
        .map(|i| {
            let t = a0 + sweep * (i as f32) / (steps as f32);
            center + Vec2::new(t.cos(), t.sin()) * radius
        })
        .collect()
}

/// A whole circle as one counter-clockwise ring.
fn sample_circle(center: Vec2, radius: f32) -> Vec<Vec2> {
    (0..CIRCLE_SEGMENTS)
        .map(|i| {
            let t = std::f32::consts::TAU * (i as f32) / (CIRCLE_SEGMENTS as f32);
            center + Vec2::new(t.cos(), t.sin()) * radius
        })
        .collect()
}

/// Segment count for an arc sweeping `sweep` radians, matching the whole-circle
/// tessellation the rest of the engine uses.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "sweep is a bounded positive angle, so the segment count is a small positive integer"
)]
fn arc_steps(sweep: f32) -> usize {
    let ideal = (CIRCLE_SEGMENTS as f32) * sweep / std::f32::consts::TAU;
    (ideal.ceil() as usize).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::SketchDoc;

    /// An axis-aligned rectangle of `w` x `h` with its lower-left at the origin.
    fn rect(d: &mut SketchDoc, w: f32, h: f32) -> [SketchId; 4] {
        let p = [
            d.add_point(Vec2::new(0.0, 0.0)),
            d.add_point(Vec2::new(w, 0.0)),
            d.add_point(Vec2::new(w, h)),
            d.add_point(Vec2::new(0.0, h)),
        ];
        for i in 0..4 {
            d.add_line(p[i], p[(i + 1) % 4]);
        }
        p
    }

    #[test]
    fn closed_rectangle_lowers_to_its_area() {
        let mut d = SketchDoc::new();
        rect(&mut d, 4.0, 3.0);
        let c = to_contours(&d).unwrap();
        assert_eq!(c.outline.len(), 4);
        assert!((c.area() - 12.0).abs() < 1e-3, "area {}", c.area());
    }

    #[test]
    fn outline_is_counter_clockwise_regardless_of_draw_order() {
        // Wind the rectangle backwards; lowering must normalise it.
        let mut d = SketchDoc::new();
        let p = [
            d.add_point(Vec2::new(0.0, 0.0)),
            d.add_point(Vec2::new(0.0, 2.0)),
            d.add_point(Vec2::new(2.0, 2.0)),
            d.add_point(Vec2::new(2.0, 0.0)),
        ];
        for i in 0..4 {
            d.add_line(p[i], p[(i + 1) % 4]);
        }
        let c = to_contours(&d).unwrap();
        assert!(
            ring_signed_area(&c.outline) > 0.0,
            "outline must be counter-clockwise"
        );
    }

    #[test]
    fn result_is_centroid_relative() {
        let mut d = SketchDoc::new();
        rect(&mut d, 4.0, 3.0);
        let c = to_contours(&d).unwrap();
        let centroid = c.centroid();
        assert!(
            centroid.length() < 1e-4,
            "expected centroid at the body origin, got {centroid:?}"
        );
    }

    #[test]
    fn construction_geometry_is_excluded_from_the_profile() {
        let mut d = SketchDoc::new();
        rect(&mut d, 2.0, 2.0);
        // A diagonal reference line across the square.
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(2.0, 2.0));
        let diag = d.add_line(a, b);
        d.mark_construction(diag);

        let c = to_contours(&d).unwrap();
        assert_eq!(c.outline.len(), 4, "construction line leaked into profile");
        assert!((c.area() - 4.0).abs() < 1e-3);
    }

    #[test]
    fn an_open_chain_is_rejected_at_the_gap() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(1.0, 0.0));
        let c = d.add_point(Vec2::new(1.0, 1.0));
        d.add_line(a, b);
        d.add_line(b, c);
        // `a` and `c` are loose ends.
        assert!(matches!(to_contours(&d), Err(LowerError::OpenProfile(_))));
    }

    #[test]
    fn an_all_construction_sketch_has_nothing_to_lower() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(1.0, 0.0));
        let l = d.add_line(a, b);
        d.mark_construction(l);
        assert_eq!(to_contours(&d), Err(LowerError::Empty));
    }

    #[test]
    fn a_circle_lowers_to_its_area() {
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::ZERO);
        d.add_circle(c, 2.0);
        let contours = to_contours(&d).unwrap();
        let exact = std::f32::consts::PI * 4.0;
        // Tessellated, so it under-reports slightly; within a percent is the
        // same tolerance the rest of the engine's circles carry.
        assert!(
            (contours.area() - exact).abs() / exact < 0.01,
            "area {} vs {exact}",
            contours.area()
        );
    }

    #[test]
    fn the_largest_loop_becomes_the_outline_and_the_rest_are_holes() {
        let mut d = SketchDoc::new();
        rect(&mut d, 6.0, 6.0);
        // A smaller square fully inside it.
        let h = [
            d.add_point(Vec2::new(2.0, 2.0)),
            d.add_point(Vec2::new(4.0, 2.0)),
            d.add_point(Vec2::new(4.0, 4.0)),
            d.add_point(Vec2::new(2.0, 4.0)),
        ];
        for i in 0..4 {
            d.add_line(h[i], h[(i + 1) % 4]);
        }

        let c = to_contours(&d).unwrap();
        assert_eq!(c.holes.len(), 1, "inner loop should be a hole");
        assert!(ring_signed_area(&c.outline) > 0.0, "outline must be CCW");
        assert!(ring_signed_area(&c.holes[0]) < 0.0, "hole must be CW");
        assert!(
            (c.area() - (36.0 - 4.0)).abs() < 1e-3,
            "net area {}",
            c.area()
        );
    }

    #[test]
    fn an_arc_contributes_curvature_not_a_straight_chord() {
        // A half-disc: a diameter line plus a semicircular arc over it.
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::new(0.0, 0.0));
        let right = d.add_point(Vec2::new(1.0, 0.0));
        let left = d.add_point(Vec2::new(-1.0, 0.0));
        d.add_arc(c, right, left);
        d.add_line(left, right);

        let contours = to_contours(&d).unwrap();
        let exact = std::f32::consts::PI / 2.0;
        assert!(
            (contours.area() - exact).abs() / exact < 0.01,
            "half-disc area {} vs {exact}",
            contours.area()
        );
        assert!(
            contours.outline.len() > 4,
            "arc should tessellate into many points, got {}",
            contours.outline.len()
        );
    }

    #[test]
    fn lowering_to_a_shape_preserves_the_contours() {
        // A five-sided loop: deliberately not a shape any analytic primitive
        // covers, so this still exercises the polygon path now that squares and
        // circles are recognised (see `recognition_tests`).
        let mut d = SketchDoc::new();
        let p: Vec<SketchId> = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(1.0, 3.0),
            Vec2::new(0.0, 2.0),
        ]
        .into_iter()
        .map(|v| d.add_point(v))
        .collect();
        for i in 0..5 {
            d.add_line(p[i], p[(i + 1) % 5]);
        }

        let c = to_contours(&d).unwrap();
        let ShapeDef::Polygon { outline, holes } = to_shape(&d).unwrap() else {
            unreachable!("an irregular loop lowers to a polygon")
        };
        assert_eq!(outline, c.outline);
        assert_eq!(holes, c.holes);
    }
}

#[cfg(test)]
mod recognition_tests {
    use super::*;
    use crate::doc::SketchDoc;

    /// An axis-aligned rectangle of `w` x `h` with its lower-left at `at`.
    fn rect_at(d: &mut SketchDoc, at: Vec2, w: f32, h: f32) -> [SketchId; 4] {
        let p = [
            d.add_point(at),
            d.add_point(at + Vec2::new(w, 0.0)),
            d.add_point(at + Vec2::new(w, h)),
            d.add_point(at + Vec2::new(0.0, h)),
        ];
        for i in 0..4 {
            d.add_line(p[i], p[(i + 1) % 4]);
        }
        p
    }

    #[test]
    fn a_lone_circle_stays_an_exact_circle() {
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::new(2.0, -3.0));
        d.add_circle(c, 1.5);

        let (shape, origin) = to_shape_with_origin(&d).expect("a circle is a profile");
        assert_eq!(
            shape,
            ShapeDef::Circle { radius: 1.5 },
            "polygonising this would make it a {CIRCLE_SEGMENTS}-gon"
        );
        assert!(
            origin.distance(Vec2::new(2.0, -3.0)) < 1e-6,
            "the pose comes from the circle's own centre, got {origin:?}"
        );
    }

    #[test]
    fn an_axis_aligned_loop_becomes_a_box() {
        let mut d = SketchDoc::new();
        rect_at(&mut d, Vec2::new(-1.0, -1.0), 4.0, 2.0);

        let (shape, origin) = to_shape_with_origin(&d).expect("a closed rectangle");
        assert_eq!(
            shape,
            ShapeDef::Box {
                width: 4.0,
                height: 2.0
            }
        );
        assert!(origin.distance(Vec2::new(1.0, 0.0)) < 1e-5, "{origin:?}");
    }

    #[test]
    fn a_rectangle_pulled_off_axis_degrades_to_a_polygon() {
        // The recognition reads solved geometry, not the tool that drew it, so
        // this has to stop being a Box the moment it stops being rectangular.
        let mut d = SketchDoc::new();
        let p = rect_at(&mut d, Vec2::ZERO, 2.0, 2.0);
        d.point_mut(p[2]).expect("corner").at = Vec2::new(2.6, 2.4);

        let (shape, _) = to_shape_with_origin(&d).expect("still a closed loop");
        assert!(
            matches!(shape, ShapeDef::Polygon { .. }),
            "a skewed quad is not a Box, got {shape:?}"
        );
    }

    #[test]
    fn a_five_sided_loop_is_a_polygon() {
        let mut d = SketchDoc::new();
        let p: Vec<SketchId> = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(1.0, 3.0),
            Vec2::new(0.0, 2.0),
        ]
        .into_iter()
        .map(|v| d.add_point(v))
        .collect();
        for i in 0..5 {
            d.add_line(p[i], p[(i + 1) % 5]);
        }

        let (shape, _) = to_shape_with_origin(&d).expect("closed");
        assert!(matches!(shape, ShapeDef::Polygon { .. }), "{shape:?}");
    }

    #[test]
    fn a_circle_beside_other_geometry_is_not_a_lone_circle() {
        // Two loops means a body with a hole, which no analytic primitive
        // covers — recognising the circle here would silently drop the rest.
        let mut d = SketchDoc::new();
        rect_at(&mut d, Vec2::new(-3.0, -3.0), 6.0, 6.0);
        let c = d.add_point(Vec2::ZERO);
        d.add_circle(c, 1.0);

        let (shape, _) = to_shape_with_origin(&d).expect("outline plus hole");
        match shape {
            ShapeDef::Polygon { holes, .. } => assert_eq!(holes.len(), 1),
            other => panic!("expected a polygon with a hole, got {other:?}"),
        }
    }

    #[test]
    fn construction_geometry_does_not_block_recognition() {
        // Reference lines are excluded from the profile, so a circle with a
        // centreline drawn across it is still just a circle.
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::ZERO);
        d.add_circle(c, 2.0);
        let a = d.add_point(Vec2::new(-2.0, 0.0));
        let b = d.add_point(Vec2::new(2.0, 0.0));
        let guide = d.add_line(a, b);
        d.mark_construction(guide);

        let (shape, _) = to_shape_with_origin(&d).expect("a circle plus a guide");
        assert_eq!(shape, ShapeDef::Circle { radius: 2.0 });
    }

    #[test]
    fn a_box_survives_a_lower_reopen_round_trip() {
        // The property that matters for re-opening: lowering to an analytic
        // primitive must not lose the sketch it came from.
        let mut d = SketchDoc::new();
        rect_at(&mut d, Vec2::ZERO, 3.0, 1.0);

        let (shape, origin) = to_shape_with_origin(&d).expect("closed");
        assert_eq!(
            shape,
            ShapeDef::Box {
                width: 3.0,
                height: 1.0
            }
        );

        // Re-open translates the stored document back into world space; the
        // sketch is untouched by lowering, so it lowers to the same thing.
        let (again, origin2) = to_shape_with_origin(&d).expect("closed");
        assert_eq!(shape, again);
        assert_eq!(origin, origin2);
    }

    #[test]
    fn a_degenerate_circle_is_an_error_not_a_zero_radius_body() {
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::ZERO);
        d.add_circle(c, 0.0);

        assert!(
            to_shape_with_origin(&d).is_err(),
            "a zero-radius circle encloses no area"
        );
    }
}
