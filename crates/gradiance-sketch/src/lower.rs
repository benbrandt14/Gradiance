//! Turn a settled sketch into authored body geometry.
//!
//! This is the **only** 2D-specific module in the crate. [`crate::doc`] and
//! [`crate::solve`] speak SolveSpace's native 3D workplane; lowering is where a
//! sketch becomes a flat [`ShapeDef`]. A future 3D backend adds a sibling here
//! rather than changing anything upstream of it.
//!
//! Lowering deliberately produces a plain [`ShapeDef::Polygon`] rather than a
//! new shape variant, so every derived consumer — colliders, meshes, snapping —
//! keeps working through the existing single discretization point.

use bevy::math::Vec2;
use gradiance_core::constants::CIRCLE_SEGMENTS;
use gradiance_geometry::contours::{Contours, ring_signed_area};
use gradiance_geometry::shape::ShapeDef;
use std::collections::HashMap;
use thiserror::Error;

use crate::doc::{SketchDoc, SketchEntity, SketchId};

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

    let mut contours = Contours {
        outline: wind(outline, true),
        holes: iter.map(|r| wind(r, false)).collect(),
    };

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

/// Lower a solved sketch straight to a [`ShapeDef`].
///
/// # Errors
///
/// As [`to_contours`].
pub fn to_shape(doc: &SketchDoc) -> Result<ShapeDef, LowerError> {
    let c = to_contours(doc)?;
    Ok(ShapeDef::Polygon {
        outline: c.outline,
        holes: c.holes,
    })
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
        SketchEntity::Arc { start, end, .. } => (start, end),
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
        let mut d = SketchDoc::new();
        rect(&mut d, 2.0, 2.0);
        let c = to_contours(&d).unwrap();
        let ShapeDef::Polygon { outline, holes } = to_shape(&d).unwrap() else {
            unreachable!("a sketch lowers to a polygon")
        };
        assert_eq!(outline, c.outline);
        assert_eq!(holes, c.holes);
    }
}
