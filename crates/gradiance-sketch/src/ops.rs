//! Modeling operations on a sketch: fillet, chamfer, trim, offset.
//!
//! These are *edits*, not constraints. They add and reshape geometry, and
//! where a CAD user would expect the relationship to survive, they leave a
//! constraint behind — a fillet is tangent to both legs afterwards, not merely
//! drawn tangent once. That is the difference between an operation that
//! produces a picture and one that produces a model.
//!
//! Pure geometry: no ECS, no solver. An operation appends to the document and
//! the solver settles it afterwards, so an operation that leaves the sketch
//! over-constrained is reported by [`crate::solve`](mod@crate::solve) rather than refused here.

use bevy::math::Vec2;
use thiserror::Error;

use crate::doc::{SketchConstraint, SketchDoc, SketchEntity, SketchId};

/// Why an operation could not be performed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OpError {
    /// The named geometry is not in the document.
    #[error("unknown geometry {0:?}")]
    Unknown(SketchId),
    /// The operation needs two lines sharing an endpoint and did not get them.
    #[error("expected two lines meeting at a shared point")]
    NotACorner,
    /// The geometry is degenerate — zero length, or legs that are collinear so
    /// there is no corner to work with.
    #[error("geometry is degenerate or collinear")]
    Degenerate,
    /// The requested size does not fit the corner it was asked to modify.
    #[error("size does not fit within the adjacent segments")]
    TooLarge,
    /// Two entities that were expected to cross do not.
    #[error("entities do not intersect")]
    NoIntersection,
}

/// The two lines meeting at `corner`, with each line's far endpoint.
struct Corner {
    line_a: SketchId,
    line_b: SketchId,
    far_a: SketchId,
    far_b: SketchId,
}

fn corner_at(doc: &SketchDoc, corner: SketchId) -> Result<Corner, OpError> {
    let mut found: Vec<(SketchId, SketchId)> = Vec::new();
    for e in &doc.entities {
        if let SketchEntity::Line { id, a, b } = *e {
            if a == corner {
                found.push((id, b));
            } else if b == corner {
                found.push((id, a));
            }
        }
    }
    match found.as_slice() {
        [(la, fa), (lb, fb)] => Ok(Corner {
            line_a: *la,
            line_b: *lb,
            far_a: *fa,
            far_b: *fb,
        }),
        _ => Err(OpError::NotACorner),
    }
}

fn at(doc: &SketchDoc, id: SketchId) -> Result<Vec2, OpError> {
    doc.point(id).map(|p| p.at).ok_or(OpError::Unknown(id))
}

/// Repoint one end of a line, leaving the other alone.
fn repoint_line(doc: &mut SketchDoc, line: SketchId, from: SketchId, to: SketchId) {
    for e in &mut doc.entities {
        if let SketchEntity::Line { id, a, b } = e
            && *id == line
        {
            if *a == from {
                *a = to;
            } else if *b == from {
                *b = to;
            }
        }
    }
}

/// Geometry shared by fillet and chamfer: the corner point, the two unit
/// directions leading away from it, and the half-angle between them.
struct Legs {
    corner_at: Vec2,
    dir_a: Vec2,
    dir_b: Vec2,
    len_a: f32,
    len_b: f32,
    half_angle: f32,
}

fn legs_of(doc: &SketchDoc, c: &Corner, corner: SketchId) -> Result<Legs, OpError> {
    let p = at(doc, corner)?;
    let va = at(doc, c.far_a)? - p;
    let vb = at(doc, c.far_b)? - p;
    let (len_a, len_b) = (va.length(), vb.length());
    if len_a < f32::EPSILON || len_b < f32::EPSILON {
        return Err(OpError::Degenerate);
    }
    let (dir_a, dir_b) = (va / len_a, vb / len_b);
    let cos = dir_a.dot(dir_b).clamp(-1.0, 1.0);
    let angle = cos.acos();
    // Collinear legs (straight through, or doubled back) have no corner.
    if angle < 1e-4 || (std::f32::consts::PI - angle).abs() < 1e-4 {
        return Err(OpError::Degenerate);
    }
    Ok(Legs {
        corner_at: p,
        dir_a,
        dir_b,
        len_a,
        len_b,
        half_angle: angle / 2.0,
    })
}

/// Round a corner where two lines meet, inserting a tangent arc.
///
/// Both legs are shortened to the tangent points and an arc is inserted
/// between them, carrying real `ArcLineTangent` constraints — so the fillet
/// stays tangent when the sketch is re-solved rather than being tangent only
/// at the moment it was created.
///
/// Returns the new arc's id.
///
/// # Errors
///
/// [`OpError`] if `corner` does not join exactly two lines, the legs are
/// collinear, or the radius does not fit.
pub fn fillet(doc: &mut SketchDoc, corner: SketchId, radius: f32) -> Result<SketchId, OpError> {
    if radius <= 0.0 {
        return Err(OpError::Degenerate);
    }
    let c = corner_at(doc, corner)?;
    let l = legs_of(doc, &c, corner)?;

    // Distance from the corner to each tangent point.
    let setback = radius / l.half_angle.tan();
    if setback >= l.len_a || setback >= l.len_b {
        return Err(OpError::TooLarge);
    }
    let ta = l.corner_at + l.dir_a * setback;
    let tb = l.corner_at + l.dir_b * setback;
    // The centre lies along the bisector, further out than the tangent points.
    let bisector = (l.dir_a + l.dir_b).normalize_or_zero();
    if bisector == Vec2::ZERO {
        return Err(OpError::Degenerate);
    }
    let centre = l.corner_at + bisector * (radius / l.half_angle.sin());

    let pa = doc.add_point(ta);
    let pb = doc.add_point(tb);
    let pc = doc.add_point(centre);

    repoint_line(doc, c.line_a, corner, pa);
    repoint_line(doc, c.line_b, corner, pb);

    // The arc runs counter-clockwise; pick the ordering that matches the
    // corner's winding so the fillet bulges outward rather than inward.
    let cross = l.dir_a.perp_dot(l.dir_b);
    let arc = if cross > 0.0 {
        doc.add_arc(pc, pa, pb)
    } else {
        doc.add_arc(pc, pb, pa)
    };

    doc.constrain(SketchConstraint::ArcLineTangent {
        arc,
        line: c.line_a,
        at_end: false,
    });
    doc.constrain(SketchConstraint::ArcLineTangent {
        arc,
        line: c.line_b,
        at_end: true,
    });
    Ok(arc)
}

/// Cut a corner off with a straight segment.
///
/// Unlike [`fillet`] this leaves no tangency to maintain — a chamfer is just a
/// line — so no constraint is recorded beyond the geometry itself.
///
/// Returns the new line's id.
///
/// # Errors
///
/// [`OpError`] if `corner` does not join exactly two lines, the legs are
/// collinear, or the setback does not fit.
pub fn chamfer(doc: &mut SketchDoc, corner: SketchId, setback: f32) -> Result<SketchId, OpError> {
    if setback <= 0.0 {
        return Err(OpError::Degenerate);
    }
    let c = corner_at(doc, corner)?;
    let l = legs_of(doc, &c, corner)?;
    if setback >= l.len_a || setback >= l.len_b {
        return Err(OpError::TooLarge);
    }

    let pa = doc.add_point(l.corner_at + l.dir_a * setback);
    let pb = doc.add_point(l.corner_at + l.dir_b * setback);
    repoint_line(doc, c.line_a, corner, pa);
    repoint_line(doc, c.line_b, corner, pb);
    Ok(doc.add_line(pa, pb))
}

/// Where two infinite lines through the given segments cross.
///
/// Returns `None` when they are parallel.
#[must_use]
pub fn line_intersection(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> Option<Vec2> {
    let r = a1 - a0;
    let s = b1 - b0;
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (b0 - a0).perp_dot(s) / denom;
    Some(a0 + r * t)
}

/// Trim or extend `line` so that the end nearer `discard` lands on `cutter`.
///
/// This is the single gesture CAD users expect: clicking the stub you want
/// gone shortens the line, and clicking past a gap lengthens it, because both
/// are "move that endpoint to the intersection". Which endpoint moves is
/// decided by which one is nearer the click, so the operation needs no mode.
///
/// # Errors
///
/// [`OpError`] if either id is not a line, or the two do not cross.
pub fn trim(
    doc: &mut SketchDoc,
    line: SketchId,
    cutter: SketchId,
    discard: Vec2,
) -> Result<(), OpError> {
    let seg = |id: SketchId| -> Result<(SketchId, SketchId), OpError> {
        match doc.entity(id) {
            Some(SketchEntity::Line { a, b, .. }) => Ok((*a, *b)),
            Some(_) => Err(OpError::NotACorner),
            None => Err(OpError::Unknown(id)),
        }
    };
    let (la, lb) = seg(line)?;
    let (ca, cb) = seg(cutter)?;
    let x = line_intersection(at(doc, la)?, at(doc, lb)?, at(doc, ca)?, at(doc, cb)?)
        .ok_or(OpError::NoIntersection)?;

    // Move whichever endpoint is nearer the click.
    let moved = if discard.distance(at(doc, la)?) <= discard.distance(at(doc, lb)?) {
        la
    } else {
        lb
    };
    if let Some(p) = doc.point_mut(moved) {
        p.at = x;
    }
    // The trimmed end now lies on the cutter, and should stay there.
    doc.constrain(SketchConstraint::PointOnLine {
        point: moved,
        line: cutter,
    });
    Ok(())
}

/// Offset a chain of connected lines by `distance`, to the left of each
/// segment's direction when positive.
///
/// Joints are resolved by intersecting adjacent offset lines, which is what
/// keeps a rectangle's offset a rectangle instead of four disconnected
/// segments with rounded gaps. Parallel neighbours (a straight run split into
/// two segments) simply share the offset point.
///
/// Returns the ids of the new lines.
///
/// # Errors
///
/// [`OpError`] if any id is not a line, or the chain is degenerate.
pub fn offset(
    doc: &mut SketchDoc,
    chain: &[SketchId],
    distance: f32,
) -> Result<Vec<SketchId>, OpError> {
    if chain.is_empty() {
        return Err(OpError::Degenerate);
    }
    // Gather each segment's offset endpoints.
    let mut segs: Vec<(Vec2, Vec2)> = Vec::with_capacity(chain.len());
    for id in chain {
        let (a, b) = match doc.entity(*id) {
            Some(SketchEntity::Line { a, b, .. }) => (*a, *b),
            Some(_) => return Err(OpError::NotACorner),
            None => return Err(OpError::Unknown(*id)),
        };
        let (pa, pb) = (at(doc, a)?, at(doc, b)?);
        let d = pb - pa;
        if d.length_squared() < f32::EPSILON {
            return Err(OpError::Degenerate);
        }
        // Left normal.
        let n = Vec2::new(-d.y, d.x).normalize() * distance;
        segs.push((pa + n, pb + n));
    }

    // Pull shared joints back together by intersecting neighbours.
    for i in 1..segs.len() {
        let (prev_a, prev_b) = segs[i - 1];
        let (cur_a, cur_b) = segs[i];
        if let Some(x) = line_intersection(prev_a, prev_b, cur_a, cur_b) {
            segs[i - 1].1 = x;
            segs[i].0 = x;
        }
        // Parallel neighbours need no correction: the offset endpoints
        // already coincide.
    }

    let mut out = Vec::with_capacity(segs.len());
    let mut prev_end: Option<SketchId> = None;
    for (i, (a, b)) in segs.iter().enumerate() {
        // Reuse the shared joint so the offset chain stays connected.
        let pa = match prev_end {
            Some(p) if i > 0 => p,
            _ => doc.add_point(*a),
        };
        let pb = doc.add_point(*b);
        out.push(doc.add_line(pa, pb));
        prev_end = Some(pb);
    }
    Ok(out)
}

/// Mark geometry as a centerline: reference-only construction geometry.
///
/// A centerline is not a distinct entity kind — it is an ordinary line flagged
/// as construction, which is why it can be snapped to and constrained against
/// while staying out of the committed profile.
pub fn make_centerline(doc: &mut SketchDoc, line: SketchId) {
    doc.mark_construction(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A right-angled corner at the origin: legs running +x and +y.
    fn right_angle() -> (SketchDoc, SketchId) {
        let mut d = SketchDoc::new();
        let corner = d.add_point(Vec2::ZERO);
        let ax = d.add_point(Vec2::new(10.0, 0.0));
        let by = d.add_point(Vec2::new(0.0, 10.0));
        d.add_line(corner, ax);
        d.add_line(corner, by);
        (d, corner)
    }

    #[test]
    fn a_fillet_inserts_a_tangent_arc_and_shortens_both_legs() {
        let (mut d, corner) = right_angle();
        let arc = fillet(&mut d, corner, 2.0).unwrap();

        let SketchEntity::Arc { center, start, .. } = *d.entity(arc).unwrap() else {
            panic!("fillet should produce an arc");
        };
        let c = d.point(center).unwrap().at;
        let s = d.point(start).unwrap().at;
        // For a 90-degree corner the centre sits at (r, r) and the tangent
        // points at (r, 0) and (0, r).
        assert!((c - Vec2::new(2.0, 2.0)).length() < 1e-3, "centre {c:?}");
        assert!(
            (c.distance(s) - 2.0).abs() < 1e-3,
            "arc radius should be the fillet radius, got {}",
            c.distance(s)
        );

        // Tangency is recorded, not merely drawn.
        let tangents = d
            .constraints
            .iter()
            .filter(|c| matches!(c, SketchConstraint::ArcLineTangent { .. }))
            .count();
        assert_eq!(tangents, 2, "a fillet is tangent to both legs");
    }

    #[test]
    fn a_fillet_too_big_for_its_legs_is_refused() {
        let (mut d, corner) = right_angle();
        // Legs are 10 long; a radius needing more setback than that cannot fit.
        assert_eq!(fillet(&mut d, corner, 50.0), Err(OpError::TooLarge));
        assert!(
            d.constraints.is_empty(),
            "a refused fillet must not leave constraints behind"
        );
    }

    #[test]
    fn a_chamfer_replaces_the_corner_with_a_plain_line() {
        let (mut d, corner) = right_angle();
        let before = d.entities.len();
        let line = chamfer(&mut d, corner, 3.0).unwrap();

        assert_eq!(d.entities.len(), before + 1, "one new segment");
        let SketchEntity::Line { a, b, .. } = *d.entity(line).unwrap() else {
            panic!("chamfer should produce a line");
        };
        let (pa, pb) = (d.point(a).unwrap().at, d.point(b).unwrap().at);
        assert!((pa - Vec2::new(3.0, 0.0)).length() < 1e-4, "{pa:?}");
        assert!((pb - Vec2::new(0.0, 3.0)).length() < 1e-4, "{pb:?}");
        assert!(
            d.constraints.is_empty(),
            "a chamfer has no tangency to maintain"
        );
    }

    #[test]
    fn corners_that_are_not_two_lines_are_refused() {
        let mut d = SketchDoc::new();
        let lone = d.add_point(Vec2::ZERO);
        let other = d.add_point(Vec2::new(1.0, 0.0));
        d.add_line(lone, other);
        // Only one line meets here.
        assert_eq!(fillet(&mut d, lone, 0.1), Err(OpError::NotACorner));
    }

    #[test]
    fn collinear_legs_have_no_corner_to_round() {
        let mut d = SketchDoc::new();
        let mid = d.add_point(Vec2::ZERO);
        let l = d.add_point(Vec2::new(-5.0, 0.0));
        let r = d.add_point(Vec2::new(5.0, 0.0));
        d.add_line(mid, l);
        d.add_line(mid, r);
        assert_eq!(fillet(&mut d, mid, 1.0), Err(OpError::Degenerate));
    }

    #[test]
    fn trim_moves_the_clicked_end_onto_the_cutter() {
        let mut d = SketchDoc::new();
        // A horizontal line overshooting a vertical cutter at x = 4.
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(10.0, 0.0));
        let line = d.add_line(a, b);
        let c0 = d.add_point(Vec2::new(4.0, -5.0));
        let c1 = d.add_point(Vec2::new(4.0, 5.0));
        let cutter = d.add_line(c0, c1);

        // Click the far stub — the end past the cutter.
        trim(&mut d, line, cutter, Vec2::new(9.0, 0.0)).unwrap();
        assert!(
            (d.point(b).unwrap().at - Vec2::new(4.0, 0.0)).length() < 1e-4,
            "the clicked end should land on the cutter, got {:?}",
            d.point(b).unwrap().at
        );
        assert!(
            (d.point(a).unwrap().at - Vec2::ZERO).length() < 1e-4,
            "the other end must not move"
        );
        assert!(
            d.constraints
                .iter()
                .any(|c| matches!(c, SketchConstraint::PointOnLine { .. })),
            "the trimmed end should stay on the cutter"
        );
    }

    #[test]
    fn trim_extends_when_the_line_falls_short() {
        let mut d = SketchDoc::new();
        // Stops at x = 2, cutter is at x = 6: the same gesture lengthens it.
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(2.0, 0.0));
        let line = d.add_line(a, b);
        let c0 = d.add_point(Vec2::new(6.0, -5.0));
        let c1 = d.add_point(Vec2::new(6.0, 5.0));
        let cutter = d.add_line(c0, c1);

        trim(&mut d, line, cutter, Vec2::new(2.1, 0.0)).unwrap();
        assert!(
            (d.point(b).unwrap().at - Vec2::new(6.0, 0.0)).length() < 1e-4,
            "trim and extend are the same gesture"
        );
    }

    #[test]
    fn parallel_entities_cannot_be_trimmed_against_each_other() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(10.0, 0.0));
        let line = d.add_line(a, b);
        let c0 = d.add_point(Vec2::new(0.0, 3.0));
        let c1 = d.add_point(Vec2::new(10.0, 3.0));
        let cutter = d.add_line(c0, c1);
        assert_eq!(
            trim(&mut d, line, cutter, Vec2::new(9.0, 0.0)),
            Err(OpError::NoIntersection)
        );
    }

    #[test]
    fn offsetting_a_corner_keeps_the_joint_closed() {
        let mut d = SketchDoc::new();
        // An L: (0,0) -> (10,0) -> (10,10).
        let p0 = d.add_point(Vec2::new(0.0, 0.0));
        let p1 = d.add_point(Vec2::new(10.0, 0.0));
        let p2 = d.add_point(Vec2::new(10.0, 10.0));
        let l0 = d.add_line(p0, p1);
        let l1 = d.add_line(p1, p2);

        let out = offset(&mut d, &[l0, l1], 1.0).unwrap();
        assert_eq!(out.len(), 2);

        // The two offset segments must still share a point, not leave a gap.
        let SketchEntity::Line { b: end0, .. } = *d.entity(out[0]).unwrap() else {
            panic!()
        };
        let SketchEntity::Line { a: start1, .. } = *d.entity(out[1]).unwrap() else {
            panic!()
        };
        assert_eq!(
            end0, start1,
            "adjacent offset segments should share the mitred joint"
        );
        // Left of +x is +y, left of +y is -x, so the mitre lands at (9, 1).
        let j = d.point(end0).unwrap().at;
        assert!((j - Vec2::new(9.0, 1.0)).length() < 1e-3, "joint at {j:?}");
    }

    #[test]
    fn offsetting_nothing_is_refused() {
        let mut d = SketchDoc::new();
        assert_eq!(offset(&mut d, &[], 1.0), Err(OpError::Degenerate));
    }

    #[test]
    fn a_centerline_is_construction_geometry() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::ZERO);
        let b = d.add_point(Vec2::new(1.0, 0.0));
        let l = d.add_line(a, b);
        assert!(!d.is_construction(l));
        make_centerline(&mut d, l);
        assert!(
            d.is_construction(l),
            "a centerline stays snappable but out of the profile"
        );
    }
}
