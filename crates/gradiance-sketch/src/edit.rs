//! Applying constraints to geometry that already exists.
//!
//! The draw-time inference in the line tool can only guess at intent as you
//! click. This module is the other half: pick some geometry, then say what you
//! *mean* about it. That is how a CAD sketch actually gets pinned down, and it
//! is why constraints have to be attachable after the fact rather than only at
//! creation.
//!
//! [`applicable`] is the important piece for the UI. Rather than offering every
//! constraint and failing on most, it reports only what the current selection
//! can support — two lines can be parallel, a point and a line cannot — so the
//! editor can grey out the rest instead of teaching by error message.
//!
//! Pure logic: no ECS, no solver. Applying a constraint only appends to the
//! document; whether it is *satisfiable* is the solver's verdict, reported
//! separately through [`crate::solve::SolveOutcome`].

use thiserror::Error;

use crate::doc::{SketchConstraint, SketchDoc, SketchEntity, SketchId};

/// A sub-object selection within one sketch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SketchSelection {
    /// Selected points, in click order.
    pub points: Vec<SketchId>,
    /// Selected entities (lines, arcs, circles), in click order.
    pub entities: Vec<SketchId>,
}

impl SketchSelection {
    /// Nothing selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.entities.is_empty()
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.points.clear();
        self.entities.clear();
    }

    /// Toggle a point in or out of the selection.
    pub fn toggle_point(&mut self, id: SketchId) {
        if let Some(i) = self.points.iter().position(|p| *p == id) {
            self.points.remove(i);
        } else {
            self.points.push(id);
        }
    }

    /// Toggle an entity in or out of the selection.
    pub fn toggle_entity(&mut self, id: SketchId) {
        if let Some(i) = self.entities.iter().position(|e| *e == id) {
            self.entities.remove(i);
        } else {
            self.entities.push(id);
        }
    }
}

/// A constraint the user can ask for, independent of which geometry it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstraintKind {
    /// Two points occupy the same place.
    Coincident,
    /// A point lies on a line.
    PointOnLine,
    /// A point lies on a circle or arc.
    PointOnCircle,
    /// A point sits at a line's midpoint.
    Midpoint,
    /// A line is horizontal.
    Horizontal,
    /// A line is vertical.
    Vertical,
    /// Two lines are parallel.
    Parallel,
    /// Two lines meet at a right angle.
    Perpendicular,
    /// An arc meets a line tangentially.
    Tangent,
    /// Two lines have equal length.
    EqualLength,
    /// Two circles or arcs have equal radius.
    EqualRadius,
    /// A dimensioned distance between two points.
    Distance,
    /// A dimensioned distance from a point to a line.
    PointLineDistance,
    /// A dimensioned diameter.
    Diameter,
    /// A dimensioned angle between two lines.
    Angle,
    /// Two points mirrored about a line.
    Symmetric,
}

impl ConstraintKind {
    /// Whether this constraint needs a numeric value from the user.
    ///
    /// Dimensions are the constraints that carry a measurement; the rest are
    /// purely relational. The editor uses this to decide whether to prompt.
    #[must_use]
    pub fn is_dimension(self) -> bool {
        matches!(
            self,
            ConstraintKind::Distance
                | ConstraintKind::PointLineDistance
                | ConstraintKind::Diameter
                | ConstraintKind::Angle
        )
    }

    /// A short label for menus and hover text.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ConstraintKind::Coincident => "Coincident",
            ConstraintKind::PointOnLine => "Point on line",
            ConstraintKind::PointOnCircle => "Point on circle",
            ConstraintKind::Midpoint => "Midpoint",
            ConstraintKind::Horizontal => "Horizontal",
            ConstraintKind::Vertical => "Vertical",
            ConstraintKind::Parallel => "Parallel",
            ConstraintKind::Perpendicular => "Perpendicular",
            ConstraintKind::Tangent => "Tangent",
            ConstraintKind::EqualLength => "Equal length",
            ConstraintKind::EqualRadius => "Equal radius",
            ConstraintKind::Distance => "Distance",
            ConstraintKind::PointLineDistance => "Point-line distance",
            ConstraintKind::Diameter => "Diameter",
            ConstraintKind::Angle => "Angle",
            ConstraintKind::Symmetric => "Symmetric",
        }
    }
}

/// Why a constraint could not be attached.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    /// The selection is the wrong shape for this constraint.
    #[error("{kind:?} does not apply to this selection")]
    NotApplicable {
        /// The constraint that was asked for.
        kind: ConstraintKind,
    },
    /// A dimension was requested without a measurement.
    #[error("{kind:?} needs a value")]
    MissingValue {
        /// The constraint that was asked for.
        kind: ConstraintKind,
    },
    /// The selection named geometry the document does not contain.
    #[error("selection references unknown geometry {0:?}")]
    Unknown(SketchId),
}

/// Which entity kind an id refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Line,
    Arc,
    Circle,
}

fn kind_of(doc: &SketchDoc, id: SketchId) -> Option<Kind> {
    match doc.entity(id)? {
        SketchEntity::Line { .. } => Some(Kind::Line),
        SketchEntity::Arc { .. } => Some(Kind::Arc),
        SketchEntity::Circle { .. } => Some(Kind::Circle),
    }
}

/// A compact description of the selection's shape, so the rules below read as
/// rules rather than as index arithmetic.
struct Shape {
    points: usize,
    lines: usize,
    /// Arcs and circles together — the things that have a radius.
    curves: usize,
    arcs: usize,
}

fn shape_of(doc: &SketchDoc, sel: &SketchSelection) -> Shape {
    let mut s = Shape {
        points: sel.points.len(),
        lines: 0,
        curves: 0,
        arcs: 0,
    };
    for e in &sel.entities {
        match kind_of(doc, *e) {
            Some(Kind::Line) => s.lines += 1,
            Some(Kind::Arc) => {
                s.curves += 1;
                s.arcs += 1;
            }
            Some(Kind::Circle) => s.curves += 1,
            None => {}
        }
    }
    s
}

/// Every constraint the current selection can support, sorted for a stable
/// menu order.
///
/// Returning the *possible* set rather than validating after the fact is what
/// lets the editor grey out inapplicable options instead of teaching by
/// failure.
#[must_use]
pub fn applicable(doc: &SketchDoc, sel: &SketchSelection) -> Vec<ConstraintKind> {
    use ConstraintKind as K;
    let s = shape_of(doc, sel);
    let mut out = Vec::new();

    // Tangency is decided once here rather than repeated across arms: a line
    // pairs with an arc, and two arcs pair with each other.
    let tangentable = s.arcs;
    match (s.points, s.lines, s.curves) {
        // Two points: coincide them, or dimension the gap.
        (2, 0, 0) => out.extend([K::Coincident, K::Distance]),
        // A point and a line.
        (1, 1, 0) => out.extend([K::PointOnLine, K::Midpoint, K::PointLineDistance]),
        // A point and a circle or arc.
        (1, 0, 1) => out.push(K::PointOnCircle),
        // Two points and a line: mirror the pair about it.
        (2, 1, 0) => out.push(K::Symmetric),
        // One line on its own.
        (0, 1, 0) => out.extend([K::Horizontal, K::Vertical]),
        // Two lines.
        (0, 2, 0) => out.extend([K::Parallel, K::Perpendicular, K::EqualLength, K::Angle]),
        // A line and one curve. Tangency needs a real arc — a full circle has
        // no endpoint for the tangency to attach to.
        (0, 1, 1) if tangentable == 1 => out.push(K::Tangent),
        // One circle or arc.
        (0, 0, 1) => out.push(K::Diameter),
        // Two circles or arcs: equal radius, and tangency when both are arcs.
        (0, 0, 2) => {
            out.push(K::EqualRadius);
            if s.arcs == 2 {
                out.push(K::Tangent);
            }
        }
        _ => {}
    }

    out.sort_unstable();
    out
}

/// Attach `kind` to `sel`, appending to the document's constraints.
///
/// `value` supplies the measurement for dimension constraints and is ignored
/// otherwise. The returned constraint is also pushed onto the document, so the
/// caller can report what was added without re-deriving it.
///
/// # Errors
///
/// [`EditError`] if the selection is the wrong shape, a dimension is missing
/// its value, or the selection names geometry the document does not have.
pub fn apply(
    doc: &mut SketchDoc,
    kind: ConstraintKind,
    sel: &SketchSelection,
    value: Option<f32>,
) -> Result<SketchConstraint, EditError> {
    use ConstraintKind as K;

    if !applicable(doc, sel).contains(&kind) {
        return Err(EditError::NotApplicable { kind });
    }
    let need = |v: Option<f32>| v.ok_or(EditError::MissingValue { kind });
    let pt = |i: usize| sel.points.get(i).copied();
    let ent = |i: usize| sel.entities.get(i).copied();

    // `applicable` already established the selection's shape, so these lookups
    // cannot fail; the error arms exist so a future rule change cannot turn a
    // mismatch into a panic.
    let miss = || EditError::NotApplicable { kind };

    // For mixed selections, find the entity of a given kind rather than
    // assuming click order.
    let line_at = |n: usize| -> Option<SketchId> {
        sel.entities
            .iter()
            .filter(|e| kind_of(doc, **e) == Some(Kind::Line))
            .nth(n)
            .copied()
    };
    let curve_at = |n: usize| -> Option<SketchId> {
        sel.entities
            .iter()
            .filter(|e| matches!(kind_of(doc, **e), Some(Kind::Arc | Kind::Circle)))
            .nth(n)
            .copied()
    };

    let c = match kind {
        K::Coincident => {
            SketchConstraint::Coincident(pt(0).ok_or_else(miss)?, pt(1).ok_or_else(miss)?)
        }
        K::Distance => SketchConstraint::Distance {
            a: pt(0).ok_or_else(miss)?,
            b: pt(1).ok_or_else(miss)?,
            d: need(value)?,
        },
        K::PointOnLine => SketchConstraint::PointOnLine {
            point: pt(0).ok_or_else(miss)?,
            line: line_at(0).ok_or_else(miss)?,
        },
        K::Midpoint => SketchConstraint::Midpoint {
            point: pt(0).ok_or_else(miss)?,
            line: line_at(0).ok_or_else(miss)?,
        },
        K::PointLineDistance => SketchConstraint::PointLineDistance {
            point: pt(0).ok_or_else(miss)?,
            line: line_at(0).ok_or_else(miss)?,
            d: need(value)?,
        },
        K::PointOnCircle => SketchConstraint::PointOnCircle {
            point: pt(0).ok_or_else(miss)?,
            circle: curve_at(0).ok_or_else(miss)?,
        },
        K::Symmetric => SketchConstraint::SymmetricAboutLine {
            a: pt(0).ok_or_else(miss)?,
            b: pt(1).ok_or_else(miss)?,
            line: line_at(0).ok_or_else(miss)?,
        },
        K::Horizontal => SketchConstraint::Horizontal(ent(0).ok_or_else(miss)?),
        K::Vertical => SketchConstraint::Vertical(ent(0).ok_or_else(miss)?),
        K::Parallel => {
            SketchConstraint::Parallel(line_at(0).ok_or_else(miss)?, line_at(1).ok_or_else(miss)?)
        }
        K::Perpendicular => SketchConstraint::Perpendicular(
            line_at(0).ok_or_else(miss)?,
            line_at(1).ok_or_else(miss)?,
        ),
        K::EqualLength => SketchConstraint::EqualLength(
            line_at(0).ok_or_else(miss)?,
            line_at(1).ok_or_else(miss)?,
        ),
        K::Angle => SketchConstraint::Angle {
            a: line_at(0).ok_or_else(miss)?,
            b: line_at(1).ok_or_else(miss)?,
            degrees: need(value)?,
        },
        K::Tangent => tangent_constraint(doc, sel).ok_or_else(miss)?,
        K::EqualRadius => SketchConstraint::EqualRadius(
            curve_at(0).ok_or_else(miss)?,
            curve_at(1).ok_or_else(miss)?,
        ),
        K::Diameter => SketchConstraint::Diameter {
            entity: curve_at(0).ok_or_else(miss)?,
            d: need(value)?,
        },
    };
    doc.constrain(c);
    Ok(c)
}

/// Resolve "make these touch smoothly" into the specific SolveSpace constraint
/// the selection calls for.
///
/// Three different constraints wear one name in the UI, because to a user they
/// are the same request: a bezier against a line, an arc against a line, or two
/// curves against each other.
fn tangent_constraint(doc: &SketchDoc, sel: &SketchSelection) -> Option<SketchConstraint> {
    let of_kind = |k: Kind| -> Vec<SketchId> {
        sel.entities
            .iter()
            .filter(|e| kind_of(doc, **e) == Some(k))
            .copied()
            .collect()
    };
    let lines = of_kind(Kind::Line);
    let arcs = of_kind(Kind::Arc);

    if let (Some(&line), Some(&arc)) = (lines.first(), arcs.first()) {
        return Some(SketchConstraint::ArcLineTangent {
            arc,
            line,
            at_end: false,
        });
    }
    // Two arcs: a smooth join between them rather than to a straight edge.
    let (&a, &b) = (arcs.first()?, arcs.get(1)?);
    Some(SketchConstraint::CurveCurveTangent {
        a,
        b,
        a_at_end: true,
        b_at_end: false,
    })
}

/// Remove the constraint at `index`, if it exists.
///
/// Over-constraining is a normal part of sketching, so removing a constraint
/// has to be as ordinary as adding one.
pub fn remove_constraint(doc: &mut SketchDoc, index: usize) -> Option<SketchConstraint> {
    (index < doc.constraints.len()).then(|| doc.constraints.remove(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    /// Two separate lines plus a circle, enough to exercise every rule.
    fn scene() -> (SketchDoc, Vec<SketchId>, Vec<SketchId>) {
        let mut d = SketchDoc::new();
        let p = vec![
            d.add_point(Vec2::new(0.0, 0.0)),
            d.add_point(Vec2::new(2.0, 0.0)),
            d.add_point(Vec2::new(0.0, 3.0)),
            d.add_point(Vec2::new(2.0, 3.5)),
            d.add_point(Vec2::new(8.0, 8.0)),
        ];
        let l1 = d.add_line(p[0], p[1]);
        let l2 = d.add_line(p[2], p[3]);
        let c1 = d.add_circle(p[4], 1.0);
        (d, p, vec![l1, l2, c1])
    }

    fn sel(points: &[SketchId], entities: &[SketchId]) -> SketchSelection {
        SketchSelection {
            points: points.to_vec(),
            entities: entities.to_vec(),
        }
    }

    #[test]
    fn two_lines_offer_the_two_line_constraints_and_nothing_else() {
        let (d, _, e) = scene();
        let got = applicable(&d, &sel(&[], &[e[0], e[1]]));
        assert_eq!(
            got,
            {
                let mut want = vec![
                    ConstraintKind::Parallel,
                    ConstraintKind::Perpendicular,
                    ConstraintKind::EqualLength,
                    ConstraintKind::Angle,
                ];
                want.sort_unstable();
                want
            },
            "two lines should offer exactly parallel/perpendicular/equal/angle"
        );
    }

    #[test]
    fn a_lone_line_offers_only_the_axis_constraints() {
        let (d, _, e) = scene();
        let got = applicable(&d, &sel(&[], &[e[0]]));
        assert_eq!(
            got,
            vec![ConstraintKind::Horizontal, ConstraintKind::Vertical]
        );
    }

    #[test]
    fn a_point_and_a_line_cannot_be_parallel() {
        let (d, p, e) = scene();
        let got = applicable(&d, &sel(&[p[4]], &[e[0]]));
        assert!(
            !got.contains(&ConstraintKind::Parallel),
            "parallel is meaningless between a point and a line, got {got:?}"
        );
        assert!(got.contains(&ConstraintKind::PointOnLine));
        assert!(got.contains(&ConstraintKind::Midpoint));
    }

    #[test]
    fn tangency_needs_a_real_arc_not_a_full_circle() {
        let (mut d, _, e) = scene();
        // Line + circle: no tangency offered.
        let got = applicable(&d, &sel(&[], &[e[0], e[2]]));
        assert!(
            !got.contains(&ConstraintKind::Tangent),
            "a full circle has no endpoint for tangency, got {got:?}"
        );

        // Line + arc: tangency is offered.
        let c = d.add_point(Vec2::new(20.0, 20.0));
        let s = d.add_point(Vec2::new(21.0, 20.0));
        let t = d.add_point(Vec2::new(20.0, 21.0));
        let arc = d.add_arc(c, s, t);
        let got = applicable(&d, &sel(&[], &[e[0], arc]));
        assert!(got.contains(&ConstraintKind::Tangent), "got {got:?}");
    }

    #[test]
    fn applying_parallel_records_it_against_the_selected_lines() {
        let (mut d, _, e) = scene();
        let c = apply(
            &mut d,
            ConstraintKind::Parallel,
            &sel(&[], &[e[0], e[1]]),
            None,
        )
        .unwrap();
        assert_eq!(c, SketchConstraint::Parallel(e[0], e[1]));
        assert_eq!(d.constraints.len(), 1);
    }

    #[test]
    fn a_dimension_without_a_value_is_refused() {
        let (mut d, p, _) = scene();
        let s = sel(&[p[0], p[1]], &[]);
        assert_eq!(
            apply(&mut d, ConstraintKind::Distance, &s, None),
            Err(EditError::MissingValue {
                kind: ConstraintKind::Distance
            })
        );
        assert!(
            d.constraints.is_empty(),
            "a refused constraint must not be recorded"
        );
    }

    #[test]
    fn a_constraint_the_selection_cannot_support_is_refused() {
        let (mut d, p, _) = scene();
        let s = sel(&[p[0], p[1]], &[]);
        assert_eq!(
            apply(&mut d, ConstraintKind::Parallel, &s, None),
            Err(EditError::NotApplicable {
                kind: ConstraintKind::Parallel
            })
        );
        assert!(d.constraints.is_empty());
    }

    #[test]
    fn mixed_selections_resolve_by_kind_not_click_order() {
        // Entity clicked first is the circle, but PointOnCircle must still
        // find it, and the line must not be mistaken for a curve.
        let (mut d, p, e) = scene();
        let s = sel(&[p[0]], &[e[2]]);
        let c = apply(&mut d, ConstraintKind::PointOnCircle, &s, None).unwrap();
        assert_eq!(
            c,
            SketchConstraint::PointOnCircle {
                point: p[0],
                circle: e[2]
            }
        );
    }

    #[test]
    fn symmetric_takes_two_points_and_the_mirror_line() {
        let (mut d, p, e) = scene();
        let s = sel(&[p[0], p[1]], &[e[1]]);
        assert!(applicable(&d, &s).contains(&ConstraintKind::Symmetric));
        let c = apply(&mut d, ConstraintKind::Symmetric, &s, None).unwrap();
        assert_eq!(
            c,
            SketchConstraint::SymmetricAboutLine {
                a: p[0],
                b: p[1],
                line: e[1]
            }
        );
    }

    #[test]
    fn dimensions_are_flagged_so_the_editor_knows_to_prompt() {
        assert!(ConstraintKind::Distance.is_dimension());
        assert!(ConstraintKind::Angle.is_dimension());
        assert!(ConstraintKind::Diameter.is_dimension());
        assert!(!ConstraintKind::Parallel.is_dimension());
        assert!(!ConstraintKind::Coincident.is_dimension());
    }

    #[test]
    fn an_empty_selection_offers_nothing() {
        let (d, _, _) = scene();
        assert!(applicable(&d, &SketchSelection::default()).is_empty());
    }

    #[test]
    fn a_constraint_can_be_removed_again() {
        let (mut d, _, e) = scene();
        apply(
            &mut d,
            ConstraintKind::Parallel,
            &sel(&[], &[e[0], e[1]]),
            None,
        )
        .unwrap();
        assert_eq!(d.constraints.len(), 1);
        assert!(remove_constraint(&mut d, 0).is_some());
        assert!(d.constraints.is_empty());
        assert!(remove_constraint(&mut d, 0).is_none(), "out of range");
    }

    #[test]
    fn toggling_selection_adds_then_removes() {
        let (_, p, e) = scene();
        let mut s = SketchSelection::default();
        s.toggle_point(p[0]);
        s.toggle_entity(e[0]);
        assert_eq!(s.points, vec![p[0]]);
        assert_eq!(s.entities, vec![e[0]]);
        s.toggle_point(p[0]);
        assert!(s.points.is_empty());
        assert!(!s.is_empty(), "the entity is still selected");
    }
}
