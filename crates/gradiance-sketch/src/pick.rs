//! Hit-testing and snap inference over a sketch.
//!
//! This is what lets a CAD editor feel like one: hovering near geometry has to
//! name *what* you are near — an endpoint, a midpoint, a circle's centre, or a
//! point projected onto a line — because the answer decides which constraint a
//! click should create. Snapping to a coordinate is not enough; snapping to an
//! **entity** is what makes the resulting sketch parametric rather than
//! merely tidy.
//!
//! Pure math: no ECS, no rendering. The tool layer supplies a cursor and a
//! tolerance in world units and gets back a candidate.

use bevy::math::Vec2;

use crate::doc::{SketchDoc, SketchEntity, SketchId};

/// What a hover landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SketchTarget {
    /// An existing sketch point.
    Point(SketchId),
    /// An entity (line, arc, circle).
    Entity(SketchId),
}

/// The kind of feature a hover found, in the order an editor prefers them.
///
/// Ordering matters: an endpoint under the cursor should win over the line it
/// belongs to, or clicking a corner would attach to the wrong thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapKind {
    /// An existing point — the strongest snap.
    Point,
    /// The midpoint of a line.
    Midpoint,
    /// The centre of a circle or arc.
    Center,
    /// A point projected onto a line, arc, or circle.
    OnEntity,
}

/// A snap candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickHit {
    /// What was hit.
    pub target: SketchTarget,
    /// The kind of feature.
    pub kind: SnapKind,
    /// The snapped position in sketch coordinates.
    pub at: Vec2,
    /// Distance from the query cursor to [`PickHit::at`].
    pub distance: f32,
}

/// Distance from `p` to the segment `a`..`b`, plus the closest point on it.
///
/// Returned separately from a plain distance because the editor needs the
/// projected position to place a snapped click.
#[must_use]
pub fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f32) {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < f32::EPSILON {
        return (a, p.distance(a));
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    let q = a + ab * t;
    (q, p.distance(q))
}

/// The best snap candidate within `tol` of `cursor`, if any.
///
/// Construction geometry participates: reference lines exist precisely so you
/// can snap to them.
#[must_use]
pub fn pick(doc: &SketchDoc, cursor: Vec2, tol: f32) -> Option<PickHit> {
    let mut best: Option<PickHit> = None;
    let mut consider = |hit: PickHit| {
        if hit.distance > tol {
            return;
        }
        // Stronger kinds win outright; ties break on proximity. This is why
        // an endpoint beats the line under the same cursor.
        let better = match &best {
            None => true,
            Some(b) => (hit.kind, hit.distance) < (b.kind, b.distance),
        };
        if better {
            best = Some(hit);
        }
    };

    for p in &doc.points {
        consider(PickHit {
            target: SketchTarget::Point(p.id),
            kind: SnapKind::Point,
            at: p.at,
            distance: cursor.distance(p.at),
        });
    }

    for e in &doc.entities {
        match *e {
            SketchEntity::Line { id, a, b } => {
                let (Some(pa), Some(pb)) = (doc.point(a), doc.point(b)) else {
                    continue;
                };
                let mid = (pa.at + pb.at) * 0.5;
                consider(PickHit {
                    target: SketchTarget::Entity(id),
                    kind: SnapKind::Midpoint,
                    at: mid,
                    distance: cursor.distance(mid),
                });
                let (q, d) = closest_on_segment(cursor, pa.at, pb.at);
                consider(PickHit {
                    target: SketchTarget::Entity(id),
                    kind: SnapKind::OnEntity,
                    at: q,
                    distance: d,
                });
            }
            SketchEntity::Circle { id, center, radius } => {
                let Some(c) = doc.point(center) else { continue };
                consider(PickHit {
                    target: SketchTarget::Entity(id),
                    kind: SnapKind::Center,
                    at: c.at,
                    distance: cursor.distance(c.at),
                });
                // Nearest point on the rim, along the centre-to-cursor ray.
                let away = cursor - c.at;
                if away.length_squared() > f32::EPSILON {
                    let q = c.at + away.normalize() * radius;
                    consider(PickHit {
                        target: SketchTarget::Entity(id),
                        kind: SnapKind::OnEntity,
                        at: q,
                        distance: cursor.distance(q),
                    });
                }
            }
            SketchEntity::Arc {
                id,
                center,
                start,
                end: _,
            } => {
                let (Some(c), Some(s)) = (doc.point(center), doc.point(start)) else {
                    continue;
                };
                consider(PickHit {
                    target: SketchTarget::Entity(id),
                    kind: SnapKind::Center,
                    at: c.at,
                    distance: cursor.distance(c.at),
                });
                let radius = s.at.distance(c.at);
                let away = cursor - c.at;
                if away.length_squared() > f32::EPSILON {
                    let q = c.at + away.normalize() * radius;
                    consider(PickHit {
                        target: SketchTarget::Entity(id),
                        kind: SnapKind::OnEntity,
                        at: q,
                        distance: cursor.distance(q),
                    });
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::SketchDoc;

    fn line_doc() -> (SketchDoc, SketchId, SketchId, SketchId) {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(4.0, 0.0));
        let l = d.add_line(a, b);
        (d, a, b, l)
    }

    #[test]
    fn an_endpoint_beats_the_line_it_belongs_to() {
        let (d, a, _, _) = line_doc();
        // Sitting right on the endpoint, which is also on the line.
        let hit = pick(&d, Vec2::new(0.02, 0.0), 0.5).unwrap();
        assert_eq!(hit.target, SketchTarget::Point(a));
        assert_eq!(hit.kind, SnapKind::Point);
    }

    #[test]
    fn the_midpoint_is_snappable_and_beats_the_edge() {
        let (d, _, _, l) = line_doc();
        let hit = pick(&d, Vec2::new(2.03, 0.01), 0.5).unwrap();
        assert_eq!(hit.target, SketchTarget::Entity(l));
        assert_eq!(hit.kind, SnapKind::Midpoint);
        assert!((hit.at - Vec2::new(2.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn hovering_along_an_edge_projects_onto_it() {
        let (d, _, _, l) = line_doc();
        let hit = pick(&d, Vec2::new(3.0, 0.1), 0.5).unwrap();
        assert_eq!(hit.target, SketchTarget::Entity(l));
        assert_eq!(hit.kind, SnapKind::OnEntity);
        assert!(
            (hit.at - Vec2::new(3.0, 0.0)).length() < 1e-5,
            "expected the projection onto the line, got {:?}",
            hit.at
        );
    }

    #[test]
    fn nothing_within_tolerance_is_no_hit() {
        let (d, _, _, _) = line_doc();
        assert!(pick(&d, Vec2::new(2.0, 9.0), 0.5).is_none());
    }

    #[test]
    fn projection_clamps_to_the_segment_rather_than_the_infinite_line() {
        let (d, _, b, _) = line_doc();
        // Well past the far end: the nearest feature is that endpoint.
        let hit = pick(&d, Vec2::new(9.0, 0.0), 20.0).unwrap();
        assert_eq!(hit.target, SketchTarget::Point(b));
    }

    #[test]
    fn a_circle_offers_its_centre_and_its_rim() {
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::ZERO);
        let circle = d.add_circle(c, 2.0);

        let centre = pick(&d, Vec2::new(0.05, 0.0), 0.5).unwrap();
        assert_eq!(centre.target, SketchTarget::Point(c));

        let rim = pick(&d, Vec2::new(2.1, 0.0), 0.5).unwrap();
        assert_eq!(rim.target, SketchTarget::Entity(circle));
        assert_eq!(rim.kind, SnapKind::OnEntity);
        assert!((rim.at - Vec2::new(2.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn construction_geometry_is_still_snappable() {
        let (mut d, _, _, l) = line_doc();
        d.mark_construction(l);
        let hit = pick(&d, Vec2::new(3.0, 0.1), 0.5).unwrap();
        assert_eq!(
            hit.target,
            SketchTarget::Entity(l),
            "reference geometry exists to be snapped to"
        );
    }
}
