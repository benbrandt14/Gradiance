//! Where each constraint should be shown, and what it should say.
//!
//! A sketch you cannot *read* is not much better than a polygon. Constraints
//! that exist only as rows in a side panel leave the author guessing which edge
//! is held horizontal and what a dimension was set to — so every constraint
//! gets a position on the geometry it names and a token to draw there.
//!
//! Pure geometry: no ECS, no rendering, no egui. The UI turns these into
//! badges; the placement rules and the wording live here where they can be
//! tested.

use bevy::math::Vec2;

use crate::doc::{SketchConstraint, SketchDoc, SketchEntity, SketchId};

/// How a constraint should read on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    /// A measurement the author set: shown as its value, and worth reading.
    Dimension,
    /// A relationship: shown as a compact token.
    Relation,
}

/// One constraint, placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Index into [`SketchDoc::constraints`], so the UI can offer to remove it
    /// and can colour the ones the solver rejected.
    pub index: usize,
    /// Where to draw, in sketch coordinates.
    pub at: Vec2,
    /// What to draw.
    pub text: String,
    /// Whether this is a measurement or a relationship.
    pub kind: AnnotationKind,
}

/// Place every constraint in `doc` on the geometry it names.
///
/// Constraints whose geometry has gone missing are skipped rather than drawn at
/// the origin — a badge floating in empty space is worse than no badge.
#[must_use]
pub fn annotations(doc: &SketchDoc) -> Vec<Annotation> {
    doc.constraints
        .iter()
        .enumerate()
        .filter_map(|(index, c)| {
            let (text, kind) = describe(*c);
            anchor(doc, *c).map(|at| Annotation {
                index,
                at,
                text,
                kind,
            })
        })
        .collect()
}

/// What a constraint says on the canvas.
///
/// Dimensions read as their value — that is the entire point of setting one.
/// Relations get a short ASCII token rather than a symbol glyph: at badge size
/// the difference between ∥ and ⊥ is a few pixels, and a missing glyph renders
/// as a replacement box, which is worse than a letter.
fn describe(c: SketchConstraint) -> (String, AnnotationKind) {
    use AnnotationKind::{Dimension, Relation};
    use SketchConstraint as K;
    match c {
        K::Distance { d, .. } | K::PointLineDistance { d, .. } => (format_length(d), Dimension),
        K::Diameter { d, .. } => (format!("⌀{}", format_length(d)), Dimension),
        K::Angle { degrees, .. } => (format!("{degrees:.1}°"), Dimension),
        K::Horizontal(_) => ("H".to_owned(), Relation),
        K::Vertical(_) => ("V".to_owned(), Relation),
        K::Parallel(..) => ("//".to_owned(), Relation),
        K::Perpendicular(..) => ("|_".to_owned(), Relation),
        K::EqualLength(..) => ("=".to_owned(), Relation),
        K::EqualRadius(..) => ("=R".to_owned(), Relation),
        K::Coincident(..) => ("+".to_owned(), Relation),
        K::Midpoint { .. } => ("MID".to_owned(), Relation),
        K::PointOnLine { .. } | K::PointOnCircle { .. } => ("ON".to_owned(), Relation),
        K::ArcLineTangent { .. } | K::CurveCurveTangent { .. } => ("TAN".to_owned(), Relation),
        K::SymmetricAboutLine { .. } => ("SYM".to_owned(), Relation),
    }
}

/// A constraint's name in prose, for lists and status lines.
///
/// The long form of the canvas token `describe` produces. Both live here so
/// that adding
/// a constraint variant means editing one module, not hunting for the second
/// place that also happened to name them all.
#[must_use]
pub fn label(c: &SketchConstraint) -> &'static str {
    use SketchConstraint as K;
    match c {
        K::Coincident(..) => "coincident",
        K::Distance { .. } => "distance",
        K::Horizontal(_) => "horizontal",
        K::Vertical(_) => "vertical",
        K::Parallel(..) => "parallel",
        K::Perpendicular(..) => "perpendicular",
        K::EqualLength(..) => "equal length",
        K::PointOnLine { .. } => "point on line",
        K::Midpoint { .. } => "midpoint",
        K::Diameter { .. } => "diameter",
        K::EqualRadius(..) => "equal radius",
        K::Angle { .. } => "angle",
        K::PointOnCircle { .. } => "point on circle",
        K::PointLineDistance { .. } => "point-line distance",
        K::ArcLineTangent { .. } => "arc tangent",
        K::CurveCurveTangent { .. } => "curve tangent",
        K::SymmetricAboutLine { .. } => "symmetric",
    }
}

/// A length in metres, at a precision that stays readable as it shrinks.
fn format_length(m: f32) -> String {
    if m.abs() >= 10.0 {
        format!("{m:.0} m")
    } else if m.abs() >= 1.0 {
        format!("{m:.2} m")
    } else {
        // Under a metre, centimetres read better than "0.05 m".
        format!("{:.1} cm", m * 100.0)
    }
}

/// Where a constraint's badge belongs: on the thing it constrains.
fn anchor(doc: &SketchDoc, c: SketchConstraint) -> Option<Vec2> {
    use SketchConstraint as K;
    match c {
        // Anything naming two points belongs on the span between them — the
        // measurement it states, or the pair it relates.
        K::Distance { a, b, .. } | K::Coincident(a, b) | K::SymmetricAboutLine { a, b, .. } => {
            Some(midpoint(doc.point(a)?.at, doc.point(b)?.at))
        }
        // A property of one edge belongs at that edge's middle.
        K::Horizontal(l) | K::Vertical(l) => entity_center(doc, l),
        K::Diameter { entity, .. } => entity_center(doc, entity),
        // Point-to-entity relations belong on the point: that is the thing
        // being held.
        K::PointOnLine { point, .. }
        | K::PointOnCircle { point, .. }
        | K::Midpoint { point, .. }
        | K::PointLineDistance { point, .. } => Some(doc.point(point)?.at),
        // Every relation between two entities sits between them, so it is
        // obvious which pair it ties together.
        K::Parallel(a, b)
        | K::Perpendicular(a, b)
        | K::EqualLength(a, b)
        | K::EqualRadius(a, b)
        | K::Angle { a, b, .. }
        | K::CurveCurveTangent { a, b, .. }
        | K::ArcLineTangent {
            arc: a, line: b, ..
        } => Some(midpoint(entity_center(doc, a)?, entity_center(doc, b)?)),
    }
}

fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    (a + b) * 0.5
}

/// A representative point on an entity — its middle, whatever kind it is.
fn entity_center(doc: &SketchDoc, id: SketchId) -> Option<Vec2> {
    let e = doc.entities.iter().find(|e| e.id() == id)?;
    match *e {
        SketchEntity::Line { a, b, .. } => Some(midpoint(doc.point(a)?.at, doc.point(b)?.at)),
        SketchEntity::Circle { center, .. } | SketchEntity::Arc { center, .. } => {
            Some(doc.point(center)?.at)
        }
    }
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
    fn a_dimension_reads_as_its_value_on_the_span_it_measures() {
        let (mut d, a, b, _) = line_doc();
        d.constrain(SketchConstraint::Distance { a, b, d: 4.0 });

        let ann = annotations(&d);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Dimension);
        assert_eq!(ann[0].text, "4.00 m");
        assert!(
            ann[0].at.distance(Vec2::new(2.0, 0.0)) < 1e-5,
            "a distance belongs at the middle of what it measures, got {:?}",
            ann[0].at
        );
    }

    #[test]
    fn an_axis_constraint_sits_on_its_edge() {
        let (mut d, _, _, l) = line_doc();
        d.constrain(SketchConstraint::Horizontal(l));

        let ann = annotations(&d);
        assert_eq!(ann[0].text, "H");
        assert_eq!(ann[0].kind, AnnotationKind::Relation);
        assert!(ann[0].at.distance(Vec2::new(2.0, 0.0)) < 1e-5);
    }

    #[test]
    fn a_relation_between_two_edges_sits_between_them() {
        let mut d = SketchDoc::new();
        let a0 = d.add_point(Vec2::new(0.0, 0.0));
        let a1 = d.add_point(Vec2::new(2.0, 0.0));
        let first = d.add_line(a0, a1);
        let b0 = d.add_point(Vec2::new(0.0, 4.0));
        let b1 = d.add_point(Vec2::new(2.0, 4.0));
        let second = d.add_line(b0, b1);
        d.constrain(SketchConstraint::Parallel(first, second));

        let ann = annotations(&d);
        assert_eq!(ann[0].text, "//");
        assert!(
            ann[0].at.distance(Vec2::new(1.0, 2.0)) < 1e-5,
            "expected the midpoint between the two edges, got {:?}",
            ann[0].at
        );
    }

    #[test]
    fn sub_metre_lengths_read_in_centimetres() {
        // "5.0 cm" beats "0.05 m" at badge size, and most sketches are small.
        assert_eq!(format_length(0.05), "5.0 cm");
        assert_eq!(format_length(1.5), "1.50 m");
        assert_eq!(format_length(42.0), "42 m");
    }

    #[test]
    fn a_constraint_naming_deleted_geometry_is_skipped_not_drawn_at_the_origin() {
        let (mut d, a, b, _) = line_doc();
        d.constrain(SketchConstraint::Distance { a, b, d: 4.0 });
        d.remove_point(b);

        assert!(
            annotations(&d).is_empty(),
            "a badge floating in empty space is worse than no badge"
        );
    }

    #[test]
    fn every_constraint_index_is_reported_so_the_ui_can_act_on_it() {
        let (mut d, a, b, l) = line_doc();
        d.constrain(SketchConstraint::Horizontal(l));
        d.constrain(SketchConstraint::Distance { a, b, d: 4.0 });

        let ann = annotations(&d);
        assert_eq!(ann.iter().map(|a| a.index).collect::<Vec<_>>(), vec![0, 1]);
    }
}
