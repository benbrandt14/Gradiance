//! The SolveSpace bridge: compile a [`SketchDoc`] into an `slvs` system, solve
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

use slvs::{
    System, constraint as sc,
    element::AsHandle,
    entity::{ArcOfCircle, Circle, Distance, EntityHandle, LineSegment, Normal, Point, Workplane},
    group::Group,
    system::{FailReason, SolveResult},
    utils::make_quaternion,
};
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
    /// A constraint referenced a circle or arc that is not in the document.
    #[error("sketch references unknown arc or circle {0:?}")]
    UnknownArc(SketchId),
    /// The solver rejected an element the document considered well-formed.
    #[error("solver rejected {what}: {detail}")]
    Rejected {
        /// The kind of element being added when the solver objected.
        what: &'static str,
        /// The solver's complaint.
        detail: String,
    },
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

/// Handle bookkeeping for one compile-and-solve pass.
#[derive(Default)]
struct Built {
    points: HashMap<SketchId, EntityHandle<Point>>,
    lines: HashMap<SketchId, EntityHandle<LineSegment>>,
    arcs: HashMap<SketchId, EntityHandle<ArcOfCircle>>,
    circles: HashMap<SketchId, EntityHandle<Circle>>,
    /// Radius parameter backing each circle, so solved radii can be read back.
    radii: HashMap<SketchId, EntityHandle<Distance>>,
    /// slvs constraint handle -> index into [`SketchDoc::constraints`].
    constraint_index: HashMap<u32, usize>,
}

impl Built {
    fn point(&self, id: SketchId) -> Result<EntityHandle<Point>, SketchError> {
        self.points
            .get(&id)
            .copied()
            .ok_or(SketchError::UnknownPoint(id))
    }

    fn line(&self, id: SketchId) -> Result<EntityHandle<LineSegment>, SketchError> {
        self.lines
            .get(&id)
            .copied()
            .ok_or(SketchError::UnknownLine(id))
    }
}

/// Build a `map_err` closure that reports a solver rejection.
fn rejected<E: std::fmt::Debug>(what: &'static str) -> impl Fn(E) -> SketchError {
    move |e| SketchError::Rejected {
        what,
        detail: format!("{e:?}"),
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
    let g_frame = sys.add_group();
    let origin = sys
        .sketch(Point::new_in_3d(g_frame, [0.0, 0.0, 0.0]))
        .map_err(rejected("workplane origin"))?;
    let normal = sys
        .sketch(Normal::new_in_3d(
            g_frame,
            make_quaternion([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ))
        .map_err(rejected("workplane normal"))?;
    let wp = sys
        .sketch(Workplane::new(g_frame, origin, normal))
        .map_err(rejected("workplane"))?;

    let g = sys.add_group();
    let built = build(&mut sys, doc, g, wp, drag)?;

    let outcome = interpret(&sys.solve(&g), &built);
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
    wp: EntityHandle<Workplane>,
    drag: Option<SketchId>,
) -> Result<Built, SketchError> {
    let mut b = Built::default();

    for p in &doc.points {
        let h = sys
            .sketch(Point::new_on_workplane(
                g,
                wp,
                [f64::from(p.at.x), f64::from(p.at.y)],
            ))
            .map_err(rejected("point"))?;
        b.points.insert(p.id, h);
    }

    // Circles need a normal lying *on* the workplane so the solver knows they
    // are in the plane rather than merely parallel to it. Built lazily, since a
    // sketch with no circles should not carry a spare entity.
    let mut plane_normal: Option<EntityHandle<Normal>> = None;

    for e in &doc.entities {
        match *e {
            SketchEntity::Line { id, a, b: b_id } => {
                let (pa, pb) = (b.point(a)?, b.point(b_id)?);
                let h = sys
                    .sketch(LineSegment::new(g, pa, pb))
                    .map_err(rejected("line"))?;
                b.lines.insert(id, h);
            }
            SketchEntity::Arc {
                id,
                center,
                start,
                end,
            } => {
                let (pc, ps, pe) = (b.point(center)?, b.point(start)?, b.point(end)?);
                let h = sys
                    .sketch(ArcOfCircle::new(g, wp, pc, ps, pe))
                    .map_err(rejected("arc"))?;
                b.arcs.insert(id, h);
            }
            SketchEntity::Circle { id, center, radius } => {
                let pc = b.point(center)?;
                let n = match plane_normal {
                    Some(n) => n,
                    None => *plane_normal.insert(
                        sys.sketch(Normal::new_on_workplane(g, wp))
                            .map_err(rejected("workplane normal"))?,
                    ),
                };
                let r = sys
                    .sketch(Distance::new(g, f64::from(radius)))
                    .map_err(rejected("circle radius"))?;
                let h = sys
                    .sketch(Circle::new(g, n, pc, r))
                    .map_err(rejected("circle"))?;
                b.circles.insert(id, h);
                b.radii.insert(id, r);
            }
        }
    }

    for (index, c) in doc.constraints.iter().enumerate() {
        let handle = add_constraint(sys, &b, g, wp, *c)?;
        b.constraint_index.insert(handle, index);
    }

    // Authored anchors are a hard constraint: the point stays where it is.
    for p in doc.points.iter().filter(|p| p.fixed) {
        let h = b.point(p.id)?;
        sys.constrain(sc::WhereDragged::new(g, h, Some(wp)))
            .map_err(rejected("fixed point"))?;
    }

    // The drag hint is deliberately *not* a constraint. SolveSpace takes it as
    // a solver preference — "favour this parameter, change it as little as
    // possible even if that means moving others more" — so a drag steers the
    // solution without ever being able to contradict a real constraint. Using
    // `WhereDragged` here instead would make dragging a constrained point
    // report an inconsistent system rather than sliding the geometry.
    if let Some(id) = drag {
        let h = b.point(id)?;
        sys.set_dragged(&h).map_err(rejected("drag hint"))?;
    }

    Ok(b)
}

/// Translate one document constraint into its SolveSpace counterpart,
/// returning the solver handle so failures can be attributed back.
fn add_constraint(
    sys: &mut System,
    b: &Built,
    g: Group,
    wp: EntityHandle<Workplane>,
    c: SketchConstraint,
) -> Result<u32, SketchError> {
    let plane = Some(wp);
    let handle = match c {
        SketchConstraint::Coincident(a, bb) => {
            sys.constrain(sc::PointsCoincident::new(
                g,
                b.point(a)?,
                b.point(bb)?,
                plane,
            ))
            .map_err(rejected("coincident"))?
            .handle
        }
        SketchConstraint::Distance { a, b: bb, d } => {
            sys.constrain(sc::PtPtDistance::new(
                g,
                b.point(a)?,
                b.point(bb)?,
                f64::from(d),
                plane,
            ))
            .map_err(rejected("distance"))?
            .handle
        }
        SketchConstraint::Horizontal(l) => {
            sys.constrain(sc::Horizontal::from_line(g, wp, b.line(l)?))
                .map_err(rejected("horizontal"))?
                .handle
        }
        SketchConstraint::Vertical(l) => {
            sys.constrain(sc::Vertical::from_line(g, wp, b.line(l)?))
                .map_err(rejected("vertical"))?
                .handle
        }
        SketchConstraint::Parallel(a, bb) => {
            sys.constrain(sc::Parallel::new(g, b.line(a)?, b.line(bb)?, plane))
                .map_err(rejected("parallel"))?
                .handle
        }
        SketchConstraint::Perpendicular(a, bb) => {
            sys.constrain(sc::Perpendicular::new(g, b.line(a)?, b.line(bb)?, plane))
                .map_err(rejected("perpendicular"))?
                .handle
        }
        SketchConstraint::EqualLength(a, bb) => {
            sys.constrain(sc::EqualLengthLines::new(g, b.line(a)?, b.line(bb)?, plane))
                .map_err(rejected("equal length"))?
                .handle
        }
        SketchConstraint::PointOnLine { point, line } => {
            sys.constrain(sc::PtOnLine::new(g, b.point(point)?, b.line(line)?, plane))
                .map_err(rejected("point on line"))?
                .handle
        }
        SketchConstraint::Midpoint { point, line } => {
            sys.constrain(sc::AtMidpoint::new(
                g,
                b.point(point)?,
                b.line(line)?,
                plane,
            ))
            .map_err(rejected("midpoint"))?
            .handle
        }
        SketchConstraint::Angle { a, b: bb, degrees } => {
            sys.constrain(sc::Angle::new(
                g,
                b.line(a)?,
                b.line(bb)?,
                f64::from(degrees),
                plane,
                false,
            ))
            .map_err(rejected("angle"))?
            .handle
        }
        SketchConstraint::Diameter { .. } | SketchConstraint::EqualRadius(..) => {
            add_radial_constraint(sys, b, g, c)?
        }
    };
    Ok(handle)
}

/// The constraints that accept *either* an arc or a full circle.
///
/// SolveSpace models these over an `AsArc` bound, so each document id has to be
/// resolved against both handle maps and the call made at the concrete type.
/// Split out of [`add_constraint`] because the four-way arc/circle pairing for
/// equal-radius dominates the function otherwise.
fn add_radial_constraint(
    sys: &mut System,
    b: &Built,
    g: Group,
    c: SketchConstraint,
) -> Result<u32, SketchError> {
    match c {
        SketchConstraint::Diameter { entity, d } => {
            let d = f64::from(d);
            if let Some(h) = b.circles.get(&entity) {
                Ok(sys
                    .constrain(sc::Diameter::new(g, *h, d))
                    .map_err(rejected("diameter"))?
                    .handle)
            } else if let Some(h) = b.arcs.get(&entity) {
                Ok(sys
                    .constrain(sc::Diameter::new(g, *h, d))
                    .map_err(rejected("diameter"))?
                    .handle)
            } else {
                Err(SketchError::UnknownArc(entity))
            }
        }
        SketchConstraint::EqualRadius(a, bb) => {
            let fail = |id: SketchId| SketchError::UnknownArc(id);
            match (b.circles.get(&a), b.arcs.get(&a)) {
                (Some(x), _) => match (b.circles.get(&bb), b.arcs.get(&bb)) {
                    (Some(y), _) => Ok(sys
                        .constrain(sc::EqualRadius::new(g, *x, *y))
                        .map_err(rejected("equal radius"))?
                        .handle),
                    (_, Some(y)) => Ok(sys
                        .constrain(sc::EqualRadius::new(g, *x, *y))
                        .map_err(rejected("equal radius"))?
                        .handle),
                    _ => Err(fail(bb)),
                },
                (_, Some(x)) => match (b.circles.get(&bb), b.arcs.get(&bb)) {
                    (Some(y), _) => Ok(sys
                        .constrain(sc::EqualRadius::new(g, *x, *y))
                        .map_err(rejected("equal radius"))?
                        .handle),
                    (_, Some(y)) => Ok(sys
                        .constrain(sc::EqualRadius::new(g, *x, *y))
                        .map_err(rejected("equal radius"))?
                        .handle),
                    _ => Err(fail(bb)),
                },
                _ => Err(fail(a)),
            }
        }
        // `add_constraint` routes only the two radial variants here.
        _ => Err(SketchError::Rejected {
            what: "radial constraint",
            detail: format!("{c:?} is not arc/circle-generic"),
        }),
    }
}

/// Fold the solver's verdict into a [`SolveOutcome`], attributing any failed
/// constraints back to their document indices.
fn interpret(result: &SolveResult, built: &Built) -> SolveOutcome {
    match result {
        SolveResult::Ok { dof } => SolveOutcome {
            status: SolveStatus::Solved,
            dof: *dof,
            failed: Vec::new(),
        },
        SolveResult::Fail {
            dof,
            reason,
            failed_constraints,
        } => {
            let mut failed: Vec<usize> = failed_constraints
                .iter()
                .filter_map(|c| built.constraint_index.get(&c.handle()).copied())
                .collect();
            failed.sort_unstable();
            SolveOutcome {
                status: match reason {
                    FailReason::Inconsistent => SolveStatus::Inconsistent,
                    FailReason::DidntConverge => SolveStatus::DidntConverge,
                    FailReason::TooManyUnknowns => SolveStatus::TooManyUnknowns,
                },
                dof: *dof,
                failed,
            }
        }
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
        if let Ok(Point::OnWorkplane { coords, .. }) = sys.entity_data(h) {
            p.at.x = coords[0] as f32;
            p.at.y = coords[1] as f32;
        }
    }

    for e in &mut doc.entities {
        if let SketchEntity::Circle { id, radius, .. } = e {
            let Some(h) = built.radii.get(id) else {
                continue;
            };
            if let Ok(d) = sys.entity_data(h) {
                *radius = d.val as f32;
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
