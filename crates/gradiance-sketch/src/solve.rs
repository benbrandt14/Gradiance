//! The SolveSpace bridge: compile a [`SketchDoc`] into a solver system, solve
//! it, and write the settled geometry back into the document.
//!
//! # Handles are ephemeral
//!
//! Solver handles are minted fresh on every call and dropped when the system
//! goes out of scope. Nothing here is stored on the document or serialized —
//! [`SketchId`] is the only identity that survives, which is the same rule that
//! keeps `StableId` rather than `Entity` in save files.
//!
//! # Dimension-agnostic by construction
//!
//! The sketch is built on a real SolveSpace workplane (an origin point plus a
//! normal), not on an assumed XY plane. A 3D construction plane is therefore a
//! different workplane rather than a different code path, and nothing in this
//! module refers to `ShapeDef` or to any physics type.

use std::collections::HashMap;

use gradiance_slvs_sys::{ConstraintDef, Entity, Group, Status, System, constraint as sc};
use thiserror::Error;

use crate::doc::{SketchConstraint, SketchDoc, SketchEntity, SketchId};

/// Why a sketch could not be handed to the solver.
///
/// These are *structural* faults in the document — a constraint naming
/// geometry that does not exist, or geometry of the wrong kind. They are
/// distinct from the solver failing to satisfy a well-formed system, which is
/// reported through [`SolveStatus`] rather than as an error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SketchError {
    /// A constraint or entity referenced a point that is not in the document.
    #[error("sketch references unknown point {0:?}")]
    UnknownPoint(SketchId),
    /// A constraint referenced a line that is not in the document.
    #[error("sketch references unknown line {0:?}")]
    UnknownLine(SketchId),
    /// A constraint referenced a circle or arc that is not in the document,
    /// or named geometry of a kind the constraint cannot accept.
    #[error("sketch references unknown arc or circle {0:?}")]
    UnknownArc(SketchId),
}

/// Whether the solver satisfied the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveStatus {
    /// Every constraint is satisfied within tolerance.
    ///
    /// This does *not* imply the sketch is fully constrained — check
    /// [`SolveOutcome::dof`] for that.
    Solved,
    /// The constraints are mutually inconsistent.
    Inconsistent,
    /// Newton's method did not converge.
    DidntConverge,
    /// The system exceeded the solver's hard limit of 2048 variables.
    TooManyUnknowns,
}

/// The result of one solve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolveOutcome {
    /// Whether the constraints were satisfied.
    pub status: SolveStatus,
    /// Remaining unconstrained degrees of freedom.
    ///
    /// Zero means fully constrained — the readout CAD users steer by.
    pub dof: i32,
    /// Indices into [`SketchDoc::constraints`] that the solver could not
    /// satisfy. Meaningful only for the document that was just solved, since
    /// the indices shift when constraints are removed.
    pub failed: Vec<usize>,
}

impl SolveOutcome {
    /// Whether every constraint was satisfied.
    pub fn is_solved(&self) -> bool {
        self.status == SolveStatus::Solved
    }

    /// Whether the sketch has no remaining freedom.
    pub fn is_fully_constrained(&self) -> bool {
        self.is_solved() && self.dof == 0
    }
}

/// What kind of geometry a document id names.
///
/// The solver's handles are untyped, so the kind travels alongside them. This
/// is what lets [`Built`] reject "tangent to a full circle" — which has no
/// endpoint to attach to — as a structural error rather than passing nonsense
/// to the solver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Line,
    Arc,
    Circle,
    Cubic,
}

/// Handle bookkeeping for one compile-and-solve pass.
#[derive(Default)]
struct Built {
    points: HashMap<SketchId, Entity>,
    entities: HashMap<SketchId, (Entity, Kind)>,
    /// Radius parameter backing each circle, so solved radii can be read back.
    radii: HashMap<SketchId, Entity>,
    /// Solver constraint handle -> index into [`SketchDoc::constraints`].
    constraint_index: HashMap<u32, usize>,
}

impl Built {
    fn point(&self, id: SketchId) -> Result<Entity, SketchError> {
        self.points
            .get(&id)
            .copied()
            .ok_or(SketchError::UnknownPoint(id))
    }

    /// Resolve `id`, requiring it to name one of `allowed`.
    ///
    /// Several constraints are generic over a *set* of kinds — equal-radius
    /// takes an arc or a circle, tangency takes an arc or a bezier — so the
    /// admissible set is the parameter rather than a single kind.
    fn of_kind(&self, id: SketchId, allowed: &[Kind]) -> Option<Entity> {
        self.entities
            .get(&id)
            .filter(|(_, k)| allowed.contains(k))
            .map(|(e, _)| *e)
    }

    fn line(&self, id: SketchId) -> Result<Entity, SketchError> {
        self.of_kind(id, &[Kind::Line])
            .ok_or(SketchError::UnknownLine(id))
    }

    /// An arc specifically — not a full circle.
    fn arc(&self, id: SketchId) -> Result<Entity, SketchError> {
        self.of_kind(id, &[Kind::Arc])
            .ok_or(SketchError::UnknownArc(id))
    }

    fn cubic(&self, id: SketchId) -> Result<Entity, SketchError> {
        self.of_kind(id, &[Kind::Cubic])
            .ok_or(SketchError::UnknownArc(id))
    }

    /// Anything with a radius: an arc or a full circle.
    fn radial(&self, id: SketchId) -> Result<Entity, SketchError> {
        self.of_kind(id, &[Kind::Arc, Kind::Circle])
            .ok_or(SketchError::UnknownArc(id))
    }

    /// Anything with two endpoints and a tangent direction at each: an arc or
    /// a bezier.
    fn curve(&self, id: SketchId) -> Result<Entity, SketchError> {
        self.of_kind(id, &[Kind::Arc, Kind::Cubic])
            .ok_or(SketchError::UnknownArc(id))
    }
}

/// Solve `doc` in place.
///
/// `drag` optionally marks the point the user is moving. SolveSpace treats this
/// as a *preference*, not a constraint — it favours that point's parameters and
/// changes them as little as it can, moving the rest of the geometry instead.
/// That is what makes dragging constrained geometry behave the way a CAD user
/// expects, and it is why the solver runs during a gesture rather than only on
/// release. Because it is a preference it can never contradict a real
/// constraint, and it is never recorded in the document.
///
/// On success the document's points and circle radii are updated to the settled
/// solution. When the solver fails the document is left **untouched**, so a
/// sketch never degrades into a half-solved state.
///
/// # Errors
///
/// Returns [`SketchError`] if the document is structurally malformed — a
/// constraint naming missing geometry, or geometry of the wrong kind.
pub fn solve(doc: &mut SketchDoc, drag: Option<SketchId>) -> Result<SolveOutcome, SketchError> {
    let mut sys = System::new();

    // Group 1 holds the workplane and is never solved, so it stays a fixed
    // reference frame for everything built into group 2.
    let g_frame = sys.group();
    let origin = sys.add_point_3d(g_frame, [0.0, 0.0, 0.0]);
    let normal = sys.add_normal_3d(
        g_frame,
        System::quaternion([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    );
    let wp = sys.add_workplane(g_frame, origin, normal);

    let g = sys.group();
    let built = build(&mut sys, doc, g, wp, drag)?;

    let outcome = interpret(&sys.solve(g), &built);
    if outcome.is_solved() {
        write_back(&sys, doc, &built);
    }
    Ok(outcome)
}

/// Populate the solver system from the document.
fn build(
    sys: &mut System,
    doc: &SketchDoc,
    g: Group,
    wp: Entity,
    drag: Option<SketchId>,
) -> Result<Built, SketchError> {
    let mut b = Built::default();

    for p in &doc.points {
        let h = sys.add_point_2d(g, wp, [f64::from(p.at.x), f64::from(p.at.y)]);
        b.points.insert(p.id, h);
    }

    // Circles need a normal lying *on* the workplane so the solver knows they
    // are in the plane rather than merely parallel to it. Built lazily, since a
    // sketch with no circles should not carry a spare entity.
    let mut plane_normal: Option<Entity> = None;

    for e in &doc.entities {
        match *e {
            SketchEntity::Line { id, a, b: b_id } => {
                let (pa, pb) = (b.point(a)?, b.point(b_id)?);
                let h = sys.add_line_2d(g, wp, pa, pb);
                b.entities.insert(id, (h, Kind::Line));
            }
            SketchEntity::Arc {
                id,
                center,
                start,
                end,
            } => {
                let (pc, ps, pe) = (b.point(center)?, b.point(start)?, b.point(end)?);
                let n = *plane_normal.get_or_insert_with(|| sys.add_normal_2d(g, wp));
                let h = sys.add_arc(g, wp, n, pc, ps, pe);
                b.entities.insert(id, (h, Kind::Arc));
            }
            SketchEntity::Cubic {
                id,
                start,
                start_control,
                end_control,
                end,
            } => {
                let pts = [
                    b.point(start)?,
                    b.point(start_control)?,
                    b.point(end_control)?,
                    b.point(end)?,
                ];
                let h = sys.add_cubic(g, wp, pts);
                b.entities.insert(id, (h, Kind::Cubic));
            }
            SketchEntity::Circle { id, center, radius } => {
                let pc = b.point(center)?;
                let n = *plane_normal.get_or_insert_with(|| sys.add_normal_2d(g, wp));
                let r = sys.add_distance(g, wp, f64::from(radius));
                let h = sys.add_circle(g, wp, n, pc, r);
                b.entities.insert(id, (h, Kind::Circle));
                b.radii.insert(id, r);
            }
        }
    }

    for (index, c) in doc.constraints.iter().enumerate() {
        let (plane, def) = constraint_def(&b, *c)?;
        let handle = sys.constrain(g, plane.unwrap_or(wp), def);
        b.constraint_index.insert(handle.handle(), index);
    }

    // Authored anchors are a hard constraint: the point stays where it is.
    for p in doc.points.iter().filter(|p| p.fixed) {
        let h = b.point(p.id)?;
        sys.constrain(
            g,
            wp,
            ConstraintDef {
                kind: sc::WHERE_DRAGGED,
                pt_a: h,
                ..Default::default()
            },
        );
    }

    // The drag hint is deliberately *not* a constraint. SolveSpace takes it as
    // a solver preference — "favour this parameter, change it as little as
    // possible even if that means moving others more" — so a drag steers the
    // solution without ever being able to contradict a real constraint. Using
    // `WHERE_DRAGGED` here instead would make dragging a constrained point
    // report an inconsistent system rather than sliding the geometry.
    if let Some(id) = drag {
        sys.drag(b.point(id)?);
    }

    Ok(b)
}

/// A constraint over two points.
fn pp(kind: i32, value: f64, a: Entity, b: Entity) -> ConstraintDef {
    ConstraintDef {
        kind,
        value,
        pt_a: a,
        pt_b: b,
        ..Default::default()
    }
}

/// A constraint relating a point to an entity — on-line, on-circle, midpoint.
fn pe(kind: i32, value: f64, point: Entity, entity: Entity) -> ConstraintDef {
    ConstraintDef {
        kind,
        value,
        pt_a: point,
        entity_a: entity,
        ..Default::default()
    }
}

/// A constraint over one entity — horizontal, vertical, diameter.
fn e1(kind: i32, value: f64, a: Entity) -> ConstraintDef {
    ConstraintDef {
        kind,
        value,
        entity_a: a,
        ..Default::default()
    }
}

/// A constraint over two entities — parallel, equal-length, tangent.
fn ee(kind: i32, value: f64, a: Entity, b: Entity) -> ConstraintDef {
    ConstraintDef {
        kind,
        value,
        entity_a: a,
        entity_b: b,
        ..Default::default()
    }
}

/// Translate one document constraint into solver operands.
///
/// Returns the workplane to measure in alongside the definition: almost
/// everything is measured in the sketch plane, but a few constraints
/// (diameter, notably) are inherently planar and upstream passes
/// `SLVS_FREE_IN_3D` for them. `None` means "the sketch plane".
///
/// Pure: it resolves handles and fills in operand slots, and touches no solver
/// state, which is what makes the whole constraint vocabulary testable without
/// running a solve.
///
/// The match is exhaustive by design — adding a [`SketchConstraint`] variant
/// must fail to compile until it is given solver operands here, rather than
/// being silently dropped at runtime.
fn constraint_def(
    b: &Built,
    c: SketchConstraint,
) -> Result<(Option<Entity>, ConstraintDef), SketchError> {
    use SketchConstraint as K;

    // The slot assignments below mirror upstream's own convenience
    // constructors in `src/slvs/lib.cpp` — that file is the authority on which
    // of ptA/ptB/entityA..D each constraint type reads.
    let def = match c {
        K::Coincident(a, bb) => pp(sc::POINTS_COINCIDENT, 0.0, b.point(a)?, b.point(bb)?),
        K::Distance { a, b: bb, d } => {
            pp(sc::PT_PT_DISTANCE, f64::from(d), b.point(a)?, b.point(bb)?)
        }
        K::PointOnLine { point, line } => pe(sc::PT_ON_LINE, 0.0, b.point(point)?, b.line(line)?),
        K::Midpoint { point, line } => pe(sc::AT_MIDPOINT, 0.0, b.point(point)?, b.line(line)?),
        K::PointLineDistance { point, line, d } => pe(
            sc::PT_LINE_DISTANCE,
            f64::from(d),
            b.point(point)?,
            b.line(line)?,
        ),
        // Generic over arc or full circle: both have a rim to sit on.
        K::PointOnCircle { point, circle } => {
            pe(sc::PT_ON_CIRCLE, 0.0, b.point(point)?, b.radial(circle)?)
        }
        K::SymmetricAboutLine { a, b: bb, line } => ConstraintDef {
            entity_a: b.line(line)?,
            ..pp(sc::SYMMETRIC_LINE, 0.0, b.point(a)?, b.point(bb)?)
        },

        K::Horizontal(l) => e1(sc::HORIZONTAL, 0.0, b.line(l)?),
        K::Vertical(l) => e1(sc::VERTICAL, 0.0, b.line(l)?),
        K::Parallel(a, bb) => ee(sc::PARALLEL, 0.0, b.line(a)?, b.line(bb)?),
        K::Perpendicular(a, bb) => ee(sc::PERPENDICULAR, 0.0, b.line(a)?, b.line(bb)?),
        K::EqualLength(a, bb) => ee(sc::EQUAL_LENGTH_LINES, 0.0, b.line(a)?, b.line(bb)?),
        K::Angle { a, b: bb, degrees } => {
            ee(sc::ANGLE, f64::from(degrees), b.line(a)?, b.line(bb)?)
        }
        K::LengthRatio { a, b: bb, ratio } => {
            ee(sc::LENGTH_RATIO, f64::from(ratio), b.line(a)?, b.line(bb)?)
        }
        K::LengthDifference {
            a,
            b: bb,
            difference,
        } => ee(
            sc::LENGTH_DIFFERENCE,
            f64::from(difference),
            b.line(a)?,
            b.line(bb)?,
        ),
        K::EqualAngle {
            a,
            b: bb,
            c: cc,
            d: dd,
        } => ConstraintDef {
            entity_c: b.line(cc)?,
            entity_d: b.line(dd)?,
            ..ee(sc::EQUAL_ANGLE, 0.0, b.line(a)?, b.line(bb)?)
        },

        // Diameter is a property of the circle itself rather than something
        // measured in a plane; upstream's `Slvs_Diameter` passes
        // SLVS_FREE_IN_3D, and so must we.
        K::Diameter { entity, d } => {
            return Ok((
                Some(Entity::NONE),
                e1(sc::DIAMETER, f64::from(d), b.radial(entity)?),
            ));
        }
        K::EqualRadius(a, bb) => ee(sc::EQUAL_RADIUS, 0.0, b.radial(a)?, b.radial(bb)?),
        // Tangency needs a real arc: a full circle has no endpoint for the
        // tangency to attach to. `other` picks which endpoint.
        K::ArcLineTangent { arc, line, at_end } => ConstraintDef {
            other: at_end,
            ..ee(sc::ARC_LINE_TANGENT, 0.0, b.arc(arc)?, b.line(line)?)
        },
        K::CubicLineTangent {
            cubic,
            line,
            at_end,
        } => ConstraintDef {
            other: at_end,
            ..ee(sc::CUBIC_LINE_TANGENT, 0.0, b.cubic(cubic)?, b.line(line)?)
        },
        K::CurveCurveTangent {
            a,
            b: bb,
            a_at_end,
            b_at_end,
        } => ConstraintDef {
            other: a_at_end,
            other2: b_at_end,
            ..ee(sc::CURVE_CURVE_TANGENT, 0.0, b.curve(a)?, b.curve(bb)?)
        },
    };
    Ok((None, def))
}

/// Fold the solver's verdict into a [`SolveOutcome`], attributing any failed
/// constraints back to their document indices.
fn interpret(solution: &gradiance_slvs_sys::Solution, built: &Built) -> SolveOutcome {
    let status = match solution.status {
        // A redundant-but-satisfiable system is still solved geometry. The
        // redundancy is worth surfacing eventually, but it is not a failure and
        // must not discard the solution.
        Status::Okay | Status::RedundantOkay => SolveStatus::Solved,
        Status::Inconsistent => SolveStatus::Inconsistent,
        Status::DidntConverge => SolveStatus::DidntConverge,
        Status::TooManyUnknowns => SolveStatus::TooManyUnknowns,
    };

    let mut failed: Vec<usize> = solution
        .failed
        .iter()
        .filter_map(|c| built.constraint_index.get(&c.handle()).copied())
        .collect();
    failed.sort_unstable();

    SolveOutcome {
        status,
        dof: solution.dof,
        failed,
    }
}

/// Copy the settled solution back into the document.
///
/// Points that the solver did not report on are left as authored rather than
/// zeroed, so a partial read can never silently collapse geometry to the
/// origin.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the document is f32; the solver works in f64 and the excess precision is not authored state"
)]
fn write_back(sys: &System, doc: &mut SketchDoc, built: &Built) {
    for p in &mut doc.points {
        let Some(h) = built.points.get(&p.id) else {
            continue;
        };
        if let Some([u, v]) = sys.point_2d(*h) {
            p.at.x = u as f32;
            p.at.y = v as f32;
        }
    }

    for e in &mut doc.entities {
        if let SketchEntity::Circle { id, radius, .. } = e {
            let Some(h) = built.radii.get(id) else {
                continue;
            };
            if let Some(r) = sys.distance_value(*h) {
                *radius = r as f32;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    /// A rectangle sketched loosely: four corners, four lines, nothing yet
    /// constrained. Returns the doc plus its corner and line ids.
    fn loose_rect() -> (SketchDoc, [SketchId; 4], [SketchId; 4]) {
        let mut d = SketchDoc::new();
        let p = [
            d.add_point(Vec2::new(0.0, 0.0)),
            d.add_point(Vec2::new(2.1, 0.3)),
            d.add_point(Vec2::new(1.8, 1.7)),
            d.add_point(Vec2::new(-0.2, 2.2)),
        ];
        let l = [
            d.add_line(p[0], p[1]),
            d.add_line(p[1], p[2]),
            d.add_line(p[2], p[3]),
            d.add_line(p[3], p[0]),
        ];
        (d, p, l)
    }

    #[test]
    fn satisfies_length_and_horizontal() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(3.0, 4.0));
        let line = d.add_line(a, b);
        d.constrain(SketchConstraint::Distance { a, b, d: 7.0 });
        d.constrain(SketchConstraint::Horizontal(line));

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");

        let (pa, pb) = (d.point(a).unwrap().at, d.point(b).unwrap().at);
        assert!(
            (pa.distance(pb) - 7.0).abs() < 1e-4,
            "length {pa:?}..{pb:?}"
        );
        assert!((pb.y - pa.y).abs() < 1e-4, "not horizontal: {pa:?} {pb:?}");
    }

    #[test]
    fn reports_remaining_degrees_of_freedom() {
        // Four free points is eight degrees of freedom.
        let (mut d, _, _) = loose_rect();
        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved());
        assert_eq!(out.dof, 8);
        assert!(!out.is_fully_constrained());
    }

    #[test]
    fn fully_constrained_rectangle_reaches_zero_dof() {
        let (mut d, p, l) = loose_rect();
        // Pin one corner, square up the sides, and dimension two of them.
        d.point_mut(p[0]).unwrap().fixed = true;
        d.constrain(SketchConstraint::Horizontal(l[0]));
        d.constrain(SketchConstraint::Vertical(l[1]));
        d.constrain(SketchConstraint::Horizontal(l[2]));
        d.constrain(SketchConstraint::Vertical(l[3]));
        d.constrain(SketchConstraint::Distance {
            a: p[0],
            b: p[1],
            d: 4.0,
        });
        d.constrain(SketchConstraint::Distance {
            a: p[1],
            b: p[2],
            d: 3.0,
        });

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");
        assert!(
            out.is_fully_constrained(),
            "expected 0 dof, got {}",
            out.dof
        );

        let at = |i: usize| d.point(p[i]).unwrap().at;
        assert!((at(0).distance(at(1)) - 4.0).abs() < 1e-4);
        assert!((at(1).distance(at(2)) - 3.0).abs() < 1e-4);
        // Opposite sides follow from the constraints, not from being authored.
        assert!((at(2).distance(at(3)) - 4.0).abs() < 1e-4);
        assert!((at(3).distance(at(0)) - 3.0).abs() < 1e-4);
    }

    #[test]
    fn contradictory_constraints_fail_without_corrupting_the_document() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(1.0, 0.0));
        d.point_mut(a).unwrap().fixed = true;
        d.point_mut(b).unwrap().fixed = true;
        // The same pair cannot be both 1 and 5 apart.
        d.constrain(SketchConstraint::Distance { a, b, d: 1.0 });
        d.constrain(SketchConstraint::Distance { a, b, d: 5.0 });

        let before = d.clone();
        let out = solve(&mut d, None).unwrap();
        assert!(!out.is_solved(), "expected failure, got {out:?}");
        assert_eq!(
            d, before,
            "a failed solve must leave the document untouched"
        );
    }

    #[test]
    fn drag_hint_steers_the_solution_without_breaking_constraints() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(1.0, 0.0));
        d.point_mut(a).unwrap().fixed = true;
        d.constrain(SketchConstraint::Distance { a, b, d: 1.0 });

        // The user has dragged `b` far past where the constraint allows. The
        // hint must bend rather than break: the distance still holds.
        d.point_mut(b).unwrap().at = Vec2::new(9.0, 0.0);
        let out = solve(&mut d, Some(b)).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");

        let (pa, pb) = (d.point(a).unwrap().at, d.point(b).unwrap().at);
        assert!((pa.distance(pb) - 1.0).abs() < 1e-4, "constraint not held");
        // The hint cannot beat a hard constraint, but it does decide which
        // way the solver resolves toward.
        assert!(pb.x > pa.x, "expected b to settle on the dragged side");
    }

    #[test]
    fn constraint_naming_missing_geometry_is_a_structural_error() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::ZERO);
        let ghost = SketchId(999);
        d.constrain(SketchConstraint::Distance {
            a,
            b: ghost,
            d: 1.0,
        });
        assert_eq!(
            solve(&mut d, None),
            Err(SketchError::UnknownPoint(ghost)),
            "a dangling reference must be reported, not silently dropped"
        );
    }

    #[test]
    fn point_line_distance_is_satisfied() {
        let mut d = SketchDoc::new();
        let a = d.add_point(Vec2::new(0.0, 0.0));
        let b = d.add_point(Vec2::new(4.0, 0.0));
        d.point_mut(a).unwrap().fixed = true;
        d.point_mut(b).unwrap().fixed = true;
        let line = d.add_line(a, b);
        let p = d.add_point(Vec2::new(2.0, 0.5));
        d.constrain(SketchConstraint::PointLineDistance {
            point: p,
            line,
            d: 3.0,
        });

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");
        // The line lies on y = 0, so the distance is just |y|.
        let y = d.point(p).unwrap().at.y;
        assert!((y.abs() - 3.0).abs() < 1e-4, "point sits at y = {y}");
    }

    #[test]
    fn length_ratio_ties_two_lines_together() {
        let mut d = SketchDoc::new();
        let o = d.add_point(Vec2::ZERO);
        d.point_mut(o).unwrap().fixed = true;
        let a1 = d.add_point(Vec2::new(2.0, 0.0));
        let l1 = d.add_line(o, a1);
        let b0 = d.add_point(Vec2::new(0.0, 1.0));
        let b1 = d.add_point(Vec2::new(1.0, 1.0));
        d.point_mut(b0).unwrap().fixed = true;
        let l2 = d.add_line(b0, b1);

        d.constrain(SketchConstraint::Distance {
            a: o,
            b: a1,
            d: 6.0,
        });
        d.constrain(SketchConstraint::LengthRatio {
            a: l1,
            b: l2,
            ratio: 3.0,
        });

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");
        let len1 = d.point(o).unwrap().at.distance(d.point(a1).unwrap().at);
        let len2 = d.point(b0).unwrap().at.distance(d.point(b1).unwrap().at);
        assert!((len1 - 6.0).abs() < 1e-4, "first line is {len1}");
        assert!((len1 / len2 - 3.0).abs() < 1e-3, "ratio is {}", len1 / len2);
    }

    #[test]
    fn symmetry_about_a_line_mirrors_a_pair() {
        let mut d = SketchDoc::new();
        // A vertical mirror line on x = 0.
        let m0 = d.add_point(Vec2::new(0.0, -1.0));
        let m1 = d.add_point(Vec2::new(0.0, 1.0));
        d.point_mut(m0).unwrap().fixed = true;
        d.point_mut(m1).unwrap().fixed = true;
        let mirror = d.add_line(m0, m1);

        let a = d.add_point(Vec2::new(2.0, 0.5));
        let b = d.add_point(Vec2::new(-1.4, 0.9));
        d.point_mut(a).unwrap().fixed = true;
        d.constrain(SketchConstraint::SymmetricAboutLine { a, b, line: mirror });

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");
        let (pa, pb) = (d.point(a).unwrap().at, d.point(b).unwrap().at);
        assert!(
            (pa.x + pb.x).abs() < 1e-4,
            "not mirrored in x: {pa:?} {pb:?}"
        );
        assert!((pa.y - pb.y).abs() < 1e-4, "y should match: {pa:?} {pb:?}");
    }

    #[test]
    fn a_point_can_be_pinned_to_a_circle() {
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::ZERO);
        d.point_mut(c).unwrap().fixed = true;
        let circle = d.add_circle(c, 2.0);
        d.constrain(SketchConstraint::Diameter {
            entity: circle,
            d: 4.0,
        });
        let p = d.add_point(Vec2::new(5.0, 0.0));
        d.constrain(SketchConstraint::PointOnCircle { point: p, circle });

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");
        let r = d.point(p).unwrap().at.length();
        assert!((r - 2.0).abs() < 1e-3, "point sits at radius {r}, want 2");
    }

    #[test]
    fn tangency_needs_a_real_arc_not_a_full_circle() {
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::ZERO);
        let circle = d.add_circle(c, 1.0);
        let a = d.add_point(Vec2::new(1.0, 0.0));
        let b = d.add_point(Vec2::new(1.0, 2.0));
        let line = d.add_line(a, b);
        d.constrain(SketchConstraint::ArcLineTangent {
            arc: circle,
            line,
            at_end: false,
        });
        // A full circle has no endpoint for the tangency to attach to, so this
        // is a structural error rather than a solver failure.
        assert_eq!(solve(&mut d, None), Err(SketchError::UnknownArc(circle)));
    }

    #[test]
    fn a_bezier_survives_a_solve_and_keeps_its_endpoints() {
        let mut d = SketchDoc::new();
        let s0 = d.add_point(Vec2::new(0.0, 0.0));
        let c0 = d.add_point(Vec2::new(1.0, 2.0));
        let c1 = d.add_point(Vec2::new(3.0, 2.0));
        let s1 = d.add_point(Vec2::new(4.0, 0.0));
        d.point_mut(s0).unwrap().fixed = true;
        d.point_mut(s1).unwrap().fixed = true;
        d.add_cubic(s0, c0, c1, s1);

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed on a bezier: {out:?}");
        assert!((d.point(s0).unwrap().at - Vec2::ZERO).length() < 1e-4);
        assert!((d.point(s1).unwrap().at - Vec2::new(4.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn a_bezier_can_be_held_tangent_to_a_line() {
        let mut d = SketchDoc::new();
        // A horizontal line the bezier must leave smoothly.
        let la = d.add_point(Vec2::new(-2.0, 0.0));
        let lb = d.add_point(Vec2::new(0.0, 0.0));
        d.point_mut(la).unwrap().fixed = true;
        d.point_mut(lb).unwrap().fixed = true;
        let line = d.add_line(la, lb);

        let c0 = d.add_point(Vec2::new(1.0, 1.5));
        let c1 = d.add_point(Vec2::new(3.0, 2.0));
        let s1 = d.add_point(Vec2::new(4.0, 0.0));
        d.point_mut(s1).unwrap().fixed = true;
        let cubic = d.add_cubic(lb, c0, c1, s1);
        d.constrain(SketchConstraint::CubicLineTangent {
            cubic,
            line,
            at_end: false,
        });

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "tangency solve failed: {out:?}");
        // Leaving the joint tangent to a horizontal line means the first
        // control point must sit level with it.
        let y = d.point(c0).unwrap().at.y;
        assert!(
            y.abs() < 1e-3,
            "the outgoing control point should be level with the line, got y = {y}"
        );
    }

    #[test]
    fn solves_a_circle_radius_from_a_diameter_constraint() {
        let mut d = SketchDoc::new();
        let c = d.add_point(Vec2::ZERO);
        d.point_mut(c).unwrap().fixed = true;
        let circle = d.add_circle(c, 1.0);
        d.constrain(SketchConstraint::Diameter {
            entity: circle,
            d: 6.0,
        });

        let out = solve(&mut d, None).unwrap();
        assert!(out.is_solved(), "solver failed: {out:?}");
        let SketchEntity::Circle { radius, .. } = d.entity(circle).copied().unwrap() else {
            unreachable!("entity is a circle")
        };
        assert!((radius - 3.0).abs() < 1e-4, "radius solved to {radius}");
    }
}
