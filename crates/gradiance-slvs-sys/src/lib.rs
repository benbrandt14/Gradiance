//! Bindings for SolveSpace's geometric constraint solver.
//!
//! This crate is the whole of Gradiance's contact with SolveSpace: it compiles
//! the vendored solver (`third_party/solvespace/`, a pristine copy of upstream
//! v3.2 — see `SOURCE.md` there) and exposes it as a safe Rust API. Nothing
//! above it links C++ or writes `unsafe`.
//!
//! # Scope
//!
//! Deliberately thin and *total*: it mirrors what `slvs.h` offers — every
//! entity kind, all 38 constraint types — and makes no decisions about which of
//! them an editor should expose. Choosing a vocabulary is the job of
//! `gradiance-sketch`, one layer up. That split is what keeps this crate
//! reusable for the assembly and 3D work the sketch layer is being kept
//! dimension-agnostic for.
//!
//! # How a solve is shaped
//!
//! Geometry is grouped, and one group is solved at a time. The conventional
//! arrangement — and the one `gradiance-sketch` uses — puts a workplane in a
//! first group that is never solved, so it acts as a fixed reference frame, and
//! the sketch geometry in a second group that is:
//!
//! ```
//! use gradiance_slvs_sys::{ConstraintDef, System, constraint};
//!
//! let mut sys = System::new();
//!
//! let frame = sys.group();
//! let origin = sys.add_point_3d(frame, [0.0, 0.0, 0.0]);
//! let normal = sys.add_normal_3d(frame, System::quaternion([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
//! let plane = sys.add_workplane(frame, origin, normal);
//!
//! let g = sys.group();
//! let a = sys.add_point_2d(g, plane, [0.0, 0.0]);
//! let b = sys.add_point_2d(g, plane, [3.0, 4.0]);
//! let line = sys.add_line_2d(g, plane, a, b);
//!
//! sys.constrain(g, plane, ConstraintDef {
//!     kind: constraint::PT_PT_DISTANCE,
//!     value: 7.0,
//!     pt_a: a,
//!     pt_b: b,
//!     ..Default::default()
//! });
//! sys.constrain(g, plane, ConstraintDef {
//!     kind: constraint::HORIZONTAL,
//!     entity_a: line,
//!     ..Default::default()
//! });
//!
//! let solution = sys.solve(g);
//! assert!(solution.is_okay());
//!
//! let (pa, pb) = (sys.point_2d(a).unwrap(), sys.point_2d(b).unwrap());
//! assert!(((pb[0] - pa[0]).abs() - 7.0).abs() < 1e-9);
//! assert!((pb[1] - pa[1]).abs() < 1e-9);
//! ```
//!
//! # Thread safety
//!
//! [`System::solve`] serializes on a process-wide lock. Upstream's `Slvs_Solve`
//! keeps its working state in file-scope C++ globals, so two solves running at
//! once would corrupt each other. Building systems is unaffected — that is all
//! Rust-side — and solving a sketch is an authoring-path operation, so the lock
//! is never on a hot path.

// The single justified exception to the workspace's `unsafe_code = "deny"`.
// FFI cannot be expressed without it, and confining it to this crate is exactly
// why this crate exists: `ffi` holds the declarations, and the only calls are
// in `System::solve` and `System::quaternion`, both of which own every pointer
// they pass.
#![allow(unsafe_code)]

use std::os::raw::c_int;
use std::sync::{Mutex, PoisonError};

pub mod ffi;

/// The `SLVS_C_*` constraint types, under names that read as English.
///
/// All 38 upstream exposes, not only those `gradiance-sketch` emits today.
pub mod constraint {
    use std::os::raw::c_int;

    /// Two points occupy the same location.
    pub const POINTS_COINCIDENT: c_int = super::ffi::SLVS_C_POINTS_COINCIDENT;
    /// Two points are a given distance apart.
    pub const PT_PT_DISTANCE: c_int = super::ffi::SLVS_C_PT_PT_DISTANCE;
    /// A point is a given distance from a plane.
    pub const PT_PLANE_DISTANCE: c_int = super::ffi::SLVS_C_PT_PLANE_DISTANCE;
    /// A point is a given distance from a line.
    pub const PT_LINE_DISTANCE: c_int = super::ffi::SLVS_C_PT_LINE_DISTANCE;
    /// A point is a given distance from a face.
    pub const PT_FACE_DISTANCE: c_int = super::ffi::SLVS_C_PT_FACE_DISTANCE;
    /// A point lies in a plane.
    pub const PT_IN_PLANE: c_int = super::ffi::SLVS_C_PT_IN_PLANE;
    /// A point lies on a line.
    pub const PT_ON_LINE: c_int = super::ffi::SLVS_C_PT_ON_LINE;
    /// A point lies on a face.
    pub const PT_ON_FACE: c_int = super::ffi::SLVS_C_PT_ON_FACE;
    /// Two lines have equal length.
    pub const EQUAL_LENGTH_LINES: c_int = super::ffi::SLVS_C_EQUAL_LENGTH_LINES;
    /// Two lines' lengths are in a given ratio.
    pub const LENGTH_RATIO: c_int = super::ffi::SLVS_C_LENGTH_RATIO;
    /// A line's length equals a point-to-line distance.
    pub const EQ_LEN_PT_LINE_D: c_int = super::ffi::SLVS_C_EQ_LEN_PT_LINE_D;
    /// Two point-to-line distances are equal.
    pub const EQ_PT_LN_DISTANCES: c_int = super::ffi::SLVS_C_EQ_PT_LN_DISTANCES;
    /// Two pairs of lines meet at equal angles.
    pub const EQUAL_ANGLE: c_int = super::ffi::SLVS_C_EQUAL_ANGLE;
    /// A line's length equals an arc's length.
    pub const EQUAL_LINE_ARC_LEN: c_int = super::ffi::SLVS_C_EQUAL_LINE_ARC_LEN;
    /// Two points are symmetric about a plane.
    pub const SYMMETRIC: c_int = super::ffi::SLVS_C_SYMMETRIC;
    /// Two points are symmetric about the workplane's horizontal axis.
    pub const SYMMETRIC_HORIZ: c_int = super::ffi::SLVS_C_SYMMETRIC_HORIZ;
    /// Two points are symmetric about the workplane's vertical axis.
    pub const SYMMETRIC_VERT: c_int = super::ffi::SLVS_C_SYMMETRIC_VERT;
    /// Two points are symmetric about a line.
    pub const SYMMETRIC_LINE: c_int = super::ffi::SLVS_C_SYMMETRIC_LINE;
    /// A point sits at a line's midpoint.
    pub const AT_MIDPOINT: c_int = super::ffi::SLVS_C_AT_MIDPOINT;
    /// A line is parallel to the workplane's horizontal axis.
    pub const HORIZONTAL: c_int = super::ffi::SLVS_C_HORIZONTAL;
    /// A line is parallel to the workplane's vertical axis.
    pub const VERTICAL: c_int = super::ffi::SLVS_C_VERTICAL;
    /// A circle or arc has a given diameter.
    pub const DIAMETER: c_int = super::ffi::SLVS_C_DIAMETER;
    /// A point lies on a circle or arc.
    pub const PT_ON_CIRCLE: c_int = super::ffi::SLVS_C_PT_ON_CIRCLE;
    /// Two normals share an orientation.
    pub const SAME_ORIENTATION: c_int = super::ffi::SLVS_C_SAME_ORIENTATION;
    /// Two lines meet at a given angle, in degrees.
    pub const ANGLE: c_int = super::ffi::SLVS_C_ANGLE;
    /// Two lines are parallel.
    pub const PARALLEL: c_int = super::ffi::SLVS_C_PARALLEL;
    /// Two lines are perpendicular.
    pub const PERPENDICULAR: c_int = super::ffi::SLVS_C_PERPENDICULAR;
    /// An arc and a line are tangent where they meet.
    pub const ARC_LINE_TANGENT: c_int = super::ffi::SLVS_C_ARC_LINE_TANGENT;
    /// A cubic and a line are tangent where they meet.
    pub const CUBIC_LINE_TANGENT: c_int = super::ffi::SLVS_C_CUBIC_LINE_TANGENT;
    /// Two circles or arcs have equal radius.
    pub const EQUAL_RADIUS: c_int = super::ffi::SLVS_C_EQUAL_RADIUS;
    /// Two points are a given distance apart, measured along a direction.
    pub const PROJ_PT_DISTANCE: c_int = super::ffi::SLVS_C_PROJ_PT_DISTANCE;
    /// A point is pinned where it sits — a hard constraint, unlike
    /// [`crate::System::drag`], which is only a solver preference.
    pub const WHERE_DRAGGED: c_int = super::ffi::SLVS_C_WHERE_DRAGGED;
    /// Two curves are tangent where they meet.
    pub const CURVE_CURVE_TANGENT: c_int = super::ffi::SLVS_C_CURVE_CURVE_TANGENT;
    /// Two lines' lengths differ by a given amount.
    pub const LENGTH_DIFFERENCE: c_int = super::ffi::SLVS_C_LENGTH_DIFFERENCE;
    /// Two arcs' lengths are in a given ratio.
    pub const ARC_ARC_LEN_RATIO: c_int = super::ffi::SLVS_C_ARC_ARC_LEN_RATIO;
    /// An arc's and a line's lengths are in a given ratio.
    pub const ARC_LINE_LEN_RATIO: c_int = super::ffi::SLVS_C_ARC_LINE_LEN_RATIO;
    /// Two arcs' lengths differ by a given amount.
    pub const ARC_ARC_DIFFERENCE: c_int = super::ffi::SLVS_C_ARC_ARC_DIFFERENCE;
    /// An arc's and a line's lengths differ by a given amount.
    pub const ARC_LINE_DIFFERENCE: c_int = super::ffi::SLVS_C_ARC_LINE_DIFFERENCE;
}

/// A group of geometry. The solver works on one group at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Group(u32);

/// A handle to a solver entity — a point, line, workplane, circle, …
///
/// [`Entity::NONE`] doubles as "free in 3D" where a workplane is expected,
/// which is what SolveSpace's C API means by a zero handle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity(u32);

impl Entity {
    /// The null handle: no entity, or "not projected into any workplane".
    pub const NONE: Self = Self(ffi::SLVS_FREE_IN_3D);

    /// The raw handle, for callers correlating solver output with their own maps.
    #[must_use]
    pub const fn handle(self) -> u32 {
        self.0
    }
}

/// A handle to a constraint, used to attribute solver failures back to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Constraint(u32);

impl Constraint {
    /// The raw handle.
    #[must_use]
    pub const fn handle(self) -> u32 {
        self.0
    }
}

/// One constraint to add, as operands rather than a 12-argument call.
///
/// Which fields matter depends on `kind`; the rest stay [`Default`]. This
/// mirrors `Slvs_Constraint` on purpose — a typed enum here would have to
/// enumerate all 38 types and would go stale the moment upstream adds one,
/// while the layer that actually cares about vocabulary (`gradiance-sketch`)
/// already has its own typed constraint enum.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ConstraintDef {
    /// One of the [`constraint`] constants.
    pub kind: c_int,
    /// The distance, angle or ratio the constraint asserts, if it takes one.
    pub value: f64,
    /// First point operand.
    pub pt_a: Entity,
    /// Second point operand.
    pub pt_b: Entity,
    /// First entity operand.
    pub entity_a: Entity,
    /// Second entity operand.
    pub entity_b: Entity,
    /// Third entity operand.
    pub entity_c: Entity,
    /// Fourth entity operand.
    pub entity_d: Entity,
    /// Picks between the two solutions some constraints admit — the reflex
    /// angle rather than the acute one, for instance.
    pub other: bool,
    /// A second such selector, for the constraints that need one.
    pub other2: bool,
}

/// Whether, and how well, the solver satisfied the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Every constraint satisfied.
    Okay,
    /// Satisfied, but some constraints were redundant.
    RedundantOkay,
    /// The constraints contradict one another.
    Inconsistent,
    /// Newton's method did not converge.
    DidntConverge,
    /// The system exceeded the solver's hard variable limit.
    TooManyUnknowns,
}

/// The outcome of one [`System::solve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Solution {
    /// Whether the constraints were satisfied.
    pub status: Status,
    /// Remaining unconstrained degrees of freedom. Zero means fully
    /// constrained — the readout CAD users steer by.
    pub dof: i32,
    /// The constraints the solver could not satisfy, when it failed and was
    /// asked to work out which. Empty on success.
    pub failed: Vec<Constraint>,
}

impl Solution {
    /// Whether the solve succeeded, redundant constraints included.
    #[must_use]
    pub fn is_okay(&self) -> bool {
        matches!(self.status, Status::Okay | Status::RedundantOkay)
    }
}

/// Upstream keeps its working state in file-scope C++ globals, so only one
/// solve may be in flight per process. Held for the duration of the FFI call
/// and nothing else.
static SOLVER: Mutex<()> = Mutex::new(());

/// A constraint system under construction.
///
/// Parameters, entities and constraints accumulate in Rust-owned `Vec`s;
/// [`System::solve`] hands pointers to them across the FFI boundary for the
/// length of one call and reads the settled values back.
#[derive(Debug, Default)]
pub struct System {
    params: Vec<ffi::SlvsParam>,
    entities: Vec<ffi::SlvsEntity>,
    constraints: Vec<ffi::SlvsConstraint>,
    dragged: Vec<ffi::SlvsHParam>,
    next_handle: u32,
    next_group: u32,
}

impl System {
    /// An empty system.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a new group. Groups are solved one at a time.
    pub fn group(&mut self) -> Group {
        self.next_group += 1;
        Group(self.next_group)
    }

    /// A unit quaternion for the frame with the given U and V basis vectors.
    ///
    /// `[1,0,0]` and `[0,1,0]` give the standard XY plane; any other pair gives
    /// a workplane in some other orientation, which is the seam that keeps
    /// callers from hard-coding 2D.
    #[must_use]
    pub fn quaternion(u: [f64; 3], v: [f64; 3]) -> [f64; 4] {
        let (mut qw, mut qx, mut qy, mut qz) = (0.0, 0.0, 0.0, 0.0);
        // SAFETY: all four out-pointers are to live locals, and the six inputs
        // are plain `f64` passed by value.
        unsafe {
            ffi::Slvs_MakeQuaternion(
                u[0],
                u[1],
                u[2],
                v[0],
                v[1],
                v[2],
                &raw mut qw,
                &raw mut qx,
                &raw mut qy,
                &raw mut qz,
            );
        }
        [qw, qx, qy, qz]
    }

    /// Allocate handles are monotonic across params, entities and constraints
    /// alike; SolveSpace keys them in separate namespaces, so sharing one
    /// counter is merely simpler, never ambiguous.
    fn next(&mut self) -> u32 {
        self.next_handle += 1;
        self.next_handle
    }

    /// Add one scalar unknown with an initial value.
    fn param(&mut self, group: Group, val: f64) -> ffi::SlvsHParam {
        let h = self.next();
        self.params.push(ffi::SlvsParam {
            h,
            group: group.0,
            val,
        });
        h
    }

    /// Push an entity built by `f`, which receives the handle being assigned.
    fn entity(
        &mut self,
        group: Group,
        type_: c_int,
        wrkpl: Entity,
        build: impl FnOnce(&mut ffi::SlvsEntity),
    ) -> Entity {
        let h = self.next();
        let mut e = ffi::SlvsEntity {
            h,
            group: group.0,
            type_,
            wrkpl: wrkpl.0,
            ..Default::default()
        };
        build(&mut e);
        self.entities.push(e);
        Entity(h)
    }

    /// A point in free space, with three parameters.
    pub fn add_point_3d(&mut self, group: Group, [x, y, z]: [f64; 3]) -> Entity {
        let p = [
            self.param(group, x),
            self.param(group, y),
            self.param(group, z),
        ];
        self.entity(group, ffi::SLVS_E_POINT_IN_3D, Entity::NONE, |e| {
            e.param[..3].copy_from_slice(&p);
        })
    }

    /// A point within `workplane`, with two parameters.
    pub fn add_point_2d(&mut self, group: Group, workplane: Entity, [u, v]: [f64; 2]) -> Entity {
        let p = [self.param(group, u), self.param(group, v)];
        self.entity(group, ffi::SLVS_E_POINT_IN_2D, workplane, |e| {
            e.param[..2].copy_from_slice(&p);
        })
    }

    /// An orientation in free space, from a unit quaternion — see
    /// [`System::quaternion`].
    pub fn add_normal_3d(&mut self, group: Group, [qw, qx, qy, qz]: [f64; 4]) -> Entity {
        let p = [
            self.param(group, qw),
            self.param(group, qx),
            self.param(group, qy),
            self.param(group, qz),
        ];
        self.entity(group, ffi::SLVS_E_NORMAL_IN_3D, Entity::NONE, |e| {
            e.param = p;
        })
    }

    /// The orientation of `workplane` itself, with no parameters of its own.
    ///
    /// Circles need one of these so the solver knows they lie *in* the plane
    /// rather than merely parallel to it.
    pub fn add_normal_2d(&mut self, group: Group, workplane: Entity) -> Entity {
        self.entity(group, ffi::SLVS_E_NORMAL_IN_2D, workplane, |_| {})
    }

    /// A free scalar length — a circle's radius, typically.
    pub fn add_distance(&mut self, group: Group, workplane: Entity, value: f64) -> Entity {
        let p = self.param(group, value);
        self.entity(group, ffi::SLVS_E_DISTANCE, workplane, |e| {
            e.param[0] = p;
        })
    }

    /// A plane, from an origin point and a normal.
    pub fn add_workplane(&mut self, group: Group, origin: Entity, normal: Entity) -> Entity {
        self.entity(group, ffi::SLVS_E_WORKPLANE, Entity::NONE, |e| {
            e.point[0] = origin.0;
            e.normal = normal.0;
        })
    }

    /// A segment between two points, within `workplane`.
    pub fn add_line_2d(&mut self, group: Group, workplane: Entity, a: Entity, b: Entity) -> Entity {
        self.entity(group, ffi::SLVS_E_LINE_SEGMENT, workplane, |e| {
            e.point[0] = a.0;
            e.point[1] = b.0;
        })
    }

    /// A segment between two points in free space.
    pub fn add_line_3d(&mut self, group: Group, a: Entity, b: Entity) -> Entity {
        self.entity(group, ffi::SLVS_E_LINE_SEGMENT, Entity::NONE, |e| {
            e.point[0] = a.0;
            e.point[1] = b.0;
        })
    }

    /// A cubic Bezier through four control points.
    pub fn add_cubic(&mut self, group: Group, workplane: Entity, points: [Entity; 4]) -> Entity {
        self.entity(group, ffi::SLVS_E_CUBIC, workplane, |e| {
            for (slot, p) in e.point.iter_mut().zip(points) {
                *slot = p.0;
            }
        })
    }

    /// An arc, from centre, start and end. Always lies in a workplane.
    pub fn add_arc(
        &mut self,
        group: Group,
        workplane: Entity,
        normal: Entity,
        center: Entity,
        start: Entity,
        end: Entity,
    ) -> Entity {
        self.entity(group, ffi::SLVS_E_ARC_OF_CIRCLE, workplane, |e| {
            e.normal = normal.0;
            e.point[0] = center.0;
            e.point[1] = start.0;
            e.point[2] = end.0;
        })
    }

    /// A full circle, from a normal, a centre and a radius distance.
    pub fn add_circle(
        &mut self,
        group: Group,
        workplane: Entity,
        normal: Entity,
        center: Entity,
        radius: Entity,
    ) -> Entity {
        self.entity(group, ffi::SLVS_E_CIRCLE, workplane, |e| {
            e.normal = normal.0;
            e.point[0] = center.0;
            e.distance = radius.0;
        })
    }

    /// Add a constraint, returning its handle so a failure can be attributed
    /// back to whatever the caller built it from.
    pub fn constrain(&mut self, group: Group, workplane: Entity, def: ConstraintDef) -> Constraint {
        let h = self.next();
        self.constraints.push(ffi::SlvsConstraint {
            h,
            group: group.0,
            type_: def.kind,
            wrkpl: workplane.0,
            val_a: def.value,
            pt_a: def.pt_a.0,
            pt_b: def.pt_b.0,
            entity_a: def.entity_a.0,
            entity_b: def.entity_b.0,
            entity_c: def.entity_c.0,
            entity_d: def.entity_d.0,
            other: c_int::from(def.other),
            other2: c_int::from(def.other2),
        });
        Constraint(h)
    }

    /// Hint that the user is moving `entity`.
    ///
    /// This is a *preference*, not a constraint: the solver favours the marked
    /// parameters and changes them as little as it can, moving the rest of the
    /// geometry instead. Because it is only a preference it can never
    /// contradict a real constraint — which is what makes dragging constrained
    /// geometry behave the way a CAD user expects. Pinning a point outright is
    /// [`constraint::WHERE_DRAGGED`], a different thing.
    ///
    /// Unknown entities are ignored, so a stale handle degrades to no hint.
    pub fn drag(&mut self, entity: Entity) {
        let Some(e) = self.entities.iter().find(|e| e.h == entity.0) else {
            return;
        };
        let params: Vec<_> = e.param.iter().copied().filter(|p| *p != 0).collect();
        self.dragged.extend(params);
    }

    /// The solved (or initial) coordinates of a 2D point.
    ///
    /// Returns `None` for a handle that is not a `POINT_IN_2D`.
    #[must_use]
    pub fn point_2d(&self, entity: Entity) -> Option<[f64; 2]> {
        let e = self.find(entity, ffi::SLVS_E_POINT_IN_2D)?;
        Some([self.value(e.param[0])?, self.value(e.param[1])?])
    }

    /// The solved (or initial) coordinates of a 3D point.
    ///
    /// Returns `None` for a handle that is not a `POINT_IN_3D`.
    #[must_use]
    pub fn point_3d(&self, entity: Entity) -> Option<[f64; 3]> {
        let e = self.find(entity, ffi::SLVS_E_POINT_IN_3D)?;
        Some([
            self.value(e.param[0])?,
            self.value(e.param[1])?,
            self.value(e.param[2])?,
        ])
    }

    /// The solved (or initial) value of a distance entity — a circle's radius.
    ///
    /// Returns `None` for a handle that is not a `DISTANCE`.
    #[must_use]
    pub fn distance_value(&self, entity: Entity) -> Option<f64> {
        let e = self.find(entity, ffi::SLVS_E_DISTANCE)?;
        self.value(e.param[0])
    }

    fn find(&self, entity: Entity, type_: c_int) -> Option<&ffi::SlvsEntity> {
        self.entities
            .iter()
            .find(|e| e.h == entity.0 && e.type_ == type_)
    }

    fn value(&self, h: ffi::SlvsHParam) -> Option<f64> {
        self.params.iter().find(|p| p.h == h).map(|p| p.val)
    }

    /// Solve `group`, writing settled values back into this system.
    ///
    /// Entities in *other* groups keep their values and act as a fixed frame of
    /// reference, which is how the workplane stays put while the sketch moves.
    ///
    /// On failure the parameters are left as the solver leaves them; callers
    /// that need an all-or-nothing update should snapshot beforehand.
    pub fn solve(&mut self, group: Group) -> Solution {
        let mut failed = vec![0_u32; self.constraints.len()];

        let mut sys = ffi::SlvsSystem {
            param: self.params.as_mut_ptr(),
            params: as_c_int(self.params.len()),
            entity: self.entities.as_mut_ptr(),
            entities: as_c_int(self.entities.len()),
            constraint: self.constraints.as_mut_ptr(),
            constraints: as_c_int(self.constraints.len()),
            dragged: self.dragged.as_mut_ptr(),
            ndragged: as_c_int(self.dragged.len()),
            // Worth the extra pass: telling someone *which* constraint broke is
            // the difference between a usable editor and a blinking error.
            calculate_faileds: 1,
            failed: failed.as_mut_ptr(),
            faileds: as_c_int(failed.len()),
            dof: 0,
            result: 0,
        };

        {
            // Poisoning would mean a previous solve panicked mid-FFI. There is
            // no Rust state to corrupt here — the guard protects C globals that
            // `Slvs_Solve` reinitialises on entry — so recovering is correct.
            let _guard = SOLVER.lock().unwrap_or_else(PoisonError::into_inner);
            // SAFETY: every pointer in `sys` is to a live Rust `Vec` that
            // outlives this call and is not aliased — `self` is borrowed
            // mutably, and `failed` is a local. Each length field is the true
            // length of the corresponding allocation, and `failed` is sized to
            // the constraint count, the maximum the solver can report. The
            // solver writes only within those bounds and only to `param[].val`
            // and the output fields.
            unsafe {
                ffi::Slvs_Solve(&raw mut sys, group.0);
            }
        }

        let status = match sys.result {
            ffi::SLVS_RESULT_OKAY => Status::Okay,
            ffi::SLVS_RESULT_REDUNDANT_OKAY => Status::RedundantOkay,
            ffi::SLVS_RESULT_DIDNT_CONVERGE => Status::DidntConverge,
            ffi::SLVS_RESULT_TOO_MANY_UNKNOWNS => Status::TooManyUnknowns,
            // Upstream only ever writes the five documented codes; treating an
            // unexpected one as inconsistent fails safe.
            _ => Status::Inconsistent,
        };

        let reported = usize::try_from(sys.faileds).unwrap_or(0).min(failed.len());
        failed.truncate(reported);

        Solution {
            status,
            dof: sys.dof,
            failed: failed.into_iter().map(Constraint).collect(),
        }
    }
}

/// Lengths cross the FFI boundary as `int`. A system that large would have blown
/// the solver's 2048-variable limit long before, so saturating is unreachable in
/// practice and still better than a wrapping cast.
fn as_c_int(n: usize) -> c_int {
    c_int::try_from(n).unwrap_or(c_int::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the standard XY workplane in its own unsolved group.
    fn xy_plane(sys: &mut System) -> Entity {
        let frame = sys.group();
        let origin = sys.add_point_3d(frame, [0.0, 0.0, 0.0]);
        let normal = sys.add_normal_3d(frame, System::quaternion([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        sys.add_workplane(frame, origin, normal)
    }

    #[test]
    fn layout_matches_the_c_header() {
        // The other half of this check lives in `src/layout_check.cpp`, which
        // asserts the same numbers against the real header at compile time.
        // Together they pin both sides of the boundary to one table.
        assert_eq!(size_of::<ffi::SlvsParam>(), 16);
        assert_eq!(std::mem::offset_of!(ffi::SlvsParam, val), 8);

        assert_eq!(size_of::<ffi::SlvsEntity>(), 56);
        assert_eq!(std::mem::offset_of!(ffi::SlvsEntity, point), 16);
        assert_eq!(std::mem::offset_of!(ffi::SlvsEntity, normal), 32);
        assert_eq!(std::mem::offset_of!(ffi::SlvsEntity, distance), 36);
        assert_eq!(std::mem::offset_of!(ffi::SlvsEntity, param), 40);

        assert_eq!(size_of::<ffi::SlvsConstraint>(), 56);
        assert_eq!(std::mem::offset_of!(ffi::SlvsConstraint, val_a), 16);
        assert_eq!(std::mem::offset_of!(ffi::SlvsConstraint, pt_a), 24);
        assert_eq!(std::mem::offset_of!(ffi::SlvsConstraint, entity_a), 32);
        assert_eq!(std::mem::offset_of!(ffi::SlvsConstraint, other), 48);
        assert_eq!(std::mem::offset_of!(ffi::SlvsConstraint, other2), 52);

        if size_of::<usize>() == 8 {
            assert_eq!(size_of::<ffi::SlvsSystem>(), 88);
            assert_eq!(std::mem::offset_of!(ffi::SlvsSystem, dragged), 48);
            assert_eq!(std::mem::offset_of!(ffi::SlvsSystem, failed), 64);
            assert_eq!(std::mem::offset_of!(ffi::SlvsSystem, dof), 76);
            assert_eq!(std::mem::offset_of!(ffi::SlvsSystem, result), 80);
        }
    }

    #[test]
    fn quaternion_of_the_standard_basis_is_the_identity() {
        let q = System::quaternion([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((q[0] - 1.0).abs() < 1e-12, "got {q:?}");
        assert!(q[1..].iter().all(|c| c.abs() < 1e-12), "got {q:?}");
    }

    #[test]
    fn a_dimensioned_horizontal_line_solves_to_that_length() {
        let mut sys = System::new();
        let plane = xy_plane(&mut sys);

        let g = sys.group();
        let a = sys.add_point_2d(g, plane, [0.0, 0.0]);
        let b = sys.add_point_2d(g, plane, [3.0, 4.0]);
        let line = sys.add_line_2d(g, plane, a, b);

        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::PT_PT_DISTANCE,
                value: 7.0,
                pt_a: a,
                pt_b: b,
                ..Default::default()
            },
        );
        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::HORIZONTAL,
                entity_a: line,
                ..Default::default()
            },
        );

        let out = sys.solve(g);
        assert_eq!(out.status, Status::Okay);
        // Two endpoints in a plane are four unknowns; a length and a direction
        // remove two, leaving the line free to slide.
        assert_eq!(out.dof, 2);

        let (pa, pb) = (
            sys.point_2d(a).expect("a is a 2d point"),
            sys.point_2d(b).expect("b is a 2d point"),
        );
        assert!(((pb[0] - pa[0]).abs() - 7.0).abs() < 1e-9, "{pa:?} {pb:?}");
        assert!((pb[1] - pa[1]).abs() < 1e-9, "{pa:?} {pb:?}");
    }

    #[test]
    fn a_fully_constrained_sketch_reports_zero_dof() {
        let mut sys = System::new();
        let plane = xy_plane(&mut sys);

        let g = sys.group();
        let a = sys.add_point_2d(g, plane, [0.0, 0.0]);
        let b = sys.add_point_2d(g, plane, [3.0, 4.0]);
        let line = sys.add_line_2d(g, plane, a, b);

        // Pin one end, fix the direction, fix the length: nothing left to move.
        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::WHERE_DRAGGED,
                pt_a: a,
                ..Default::default()
            },
        );
        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::HORIZONTAL,
                entity_a: line,
                ..Default::default()
            },
        );
        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::PT_PT_DISTANCE,
                value: 5.0,
                pt_a: a,
                pt_b: b,
                ..Default::default()
            },
        );

        let out = sys.solve(g);
        assert!(out.is_okay(), "{out:?}");
        assert_eq!(
            out.dof, 0,
            "fully constrained sketch should have no freedom"
        );
    }

    #[test]
    fn contradictory_constraints_are_reported_not_silently_dropped() {
        let mut sys = System::new();
        let plane = xy_plane(&mut sys);

        let g = sys.group();
        let a = sys.add_point_2d(g, plane, [0.0, 0.0]);
        let b = sys.add_point_2d(g, plane, [3.0, 0.0]);

        // The same pair of points cannot be both 5 and 9 apart.
        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::PT_PT_DISTANCE,
                value: 5.0,
                pt_a: a,
                pt_b: b,
                ..Default::default()
            },
        );
        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::PT_PT_DISTANCE,
                value: 9.0,
                pt_a: a,
                pt_b: b,
                ..Default::default()
            },
        );

        let out = sys.solve(g);
        assert!(!out.is_okay(), "expected a failure, got {out:?}");
        assert!(
            !out.failed.is_empty(),
            "the editor needs to know which constraint broke, got {out:?}"
        );
    }

    #[test]
    fn a_circles_radius_is_solved_through_its_distance_entity() {
        let mut sys = System::new();
        let plane = xy_plane(&mut sys);

        let g = sys.group();
        let normal = sys.add_normal_2d(g, plane);
        let center = sys.add_point_2d(g, plane, [0.0, 0.0]);
        let radius = sys.add_distance(g, plane, 1.0);
        let circle = sys.add_circle(g, plane, normal, center, radius);

        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::DIAMETER,
                value: 6.0,
                entity_a: circle,
                ..Default::default()
            },
        );

        let out = sys.solve(g);
        assert!(out.is_okay(), "{out:?}");
        let r = sys.distance_value(radius).expect("radius is a distance");
        assert!((r - 3.0).abs() < 1e-9, "diameter 6 means radius 3, got {r}");
    }

    #[test]
    fn dragging_slides_constrained_geometry_instead_of_breaking_it() {
        let mut sys = System::new();
        let plane = xy_plane(&mut sys);

        let g = sys.group();
        let a = sys.add_point_2d(g, plane, [0.0, 0.0]);
        let b = sys.add_point_2d(g, plane, [4.0, 0.0]);
        let line = sys.add_line_2d(g, plane, a, b);

        sys.constrain(
            g,
            plane,
            ConstraintDef {
                kind: constraint::HORIZONTAL,
                entity_a: line,
                ..Default::default()
            },
        );

        // Pull `b` off the axis and mark it as the point being dragged. A drag
        // is a preference, so the horizontal constraint still wins — the line
        // moves rather than reporting an inconsistency.
        sys.drag(b);
        let out = sys.solve(g);

        assert!(
            out.is_okay(),
            "a drag must never break a constraint: {out:?}"
        );
        let (pa, pb) = (
            sys.point_2d(a).expect("a is a 2d point"),
            sys.point_2d(b).expect("b is a 2d point"),
        );
        assert!(
            (pb[1] - pa[1]).abs() < 1e-9,
            "still horizontal: {pa:?} {pb:?}"
        );
    }

    #[test]
    fn a_stale_drag_handle_is_ignored_rather_than_panicking() {
        let mut sys = System::new();
        let plane = xy_plane(&mut sys);
        let g = sys.group();
        let a = sys.add_point_2d(g, plane, [0.0, 0.0]);

        sys.drag(Entity(9999));
        let out = sys.solve(g);

        assert!(out.is_okay(), "{out:?}");
        assert!(sys.point_2d(a).is_some());
    }

    #[test]
    fn reading_an_entity_as_the_wrong_kind_is_none_not_nonsense() {
        let mut sys = System::new();
        let plane = xy_plane(&mut sys);
        let g = sys.group();
        let a = sys.add_point_2d(g, plane, [1.0, 2.0]);

        assert_eq!(sys.point_2d(a), Some([1.0, 2.0]));
        assert_eq!(sys.point_3d(a), None, "a 2d point is not a 3d point");
        assert_eq!(sys.distance_value(a), None, "a point is not a distance");
    }
}
