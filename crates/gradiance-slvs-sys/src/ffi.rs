//! Raw declarations for SolveSpace's C API (`third_party/solvespace/include/slvs.h`).
//!
//! Hand-written on purpose. The header is 516 lines of POD structs, integer
//! constants and six functions — small enough that transcribing it costs less
//! than a `bindgen` dependency, and the transcription buys real things: no
//! `libclang` on any build machine, no generated-code step to debug when a
//! toolchain moves under us, and constants that carry doc comments.
//!
//! The layouts here must match `slvs.h` exactly. `layout_matches_the_c_header`
//! in the crate's tests asks the C compiler for the real sizes and offsets and
//! compares them against these structs, so a drift on an upstream bump fails
//! the build rather than corrupting the solve.

// The constants below are transcribed verbatim from `slvs.h`, digit for digit,
// so the two files can be diffed by eye when upstream moves. Regrouping them as
// `100_027` would read marginally better in isolation and make that audit
// harder, which is the wrong trade for hand-written bindings.
#![allow(clippy::unreadable_literal)]

use std::os::raw::c_int;

/// A solver parameter handle — one scalar unknown.
pub type SlvsHParam = u32;
/// An entity handle (point, line, workplane, …).
pub type SlvsHEntity = u32;
/// A constraint handle.
pub type SlvsHConstraint = u32;
/// A group handle. Geometry is solved one group at a time.
pub type SlvsHGroup = u32;

/// Passed where a workplane is expected to mean "in free space, not projected".
pub const SLVS_FREE_IN_3D: SlvsHEntity = 0;

// -- Entity types ----------------------------------------------------------

/// A point with three parameters (x, y, z).
pub const SLVS_E_POINT_IN_3D: c_int = 50000;
/// A point with two parameters (u, v) within a workplane.
pub const SLVS_E_POINT_IN_2D: c_int = 50001;
/// An orientation, given as a unit quaternion (four parameters).
pub const SLVS_E_NORMAL_IN_3D: c_int = 60000;
/// An orientation that lies on a workplane, and so has no parameters of its own.
pub const SLVS_E_NORMAL_IN_2D: c_int = 60001;
/// A scalar length — a circle's radius, for instance.
pub const SLVS_E_DISTANCE: c_int = 70000;
/// A plane, given by an origin point and a normal.
pub const SLVS_E_WORKPLANE: c_int = 80000;
/// A segment between two points.
pub const SLVS_E_LINE_SEGMENT: c_int = 80001;
/// A cubic Bezier through four control points.
pub const SLVS_E_CUBIC: c_int = 80002;
/// A full circle: normal, centre, radius.
pub const SLVS_E_CIRCLE: c_int = 80003;
/// An arc: normal, centre, start, end. Always lies in a workplane.
pub const SLVS_E_ARC_OF_CIRCLE: c_int = 80004;

// -- Constraint types ------------------------------------------------------
//
// The full set upstream exposes, not only the subset `gradiance-sketch`
// currently emits. This crate is meant to be a faithful, total view of the
// kernel; deciding which constraints an editor offers is a layer up.

/// Two points are at the same location.
pub const SLVS_C_POINTS_COINCIDENT: c_int = 100000;
/// Two points are a given distance apart.
pub const SLVS_C_PT_PT_DISTANCE: c_int = 100001;
/// A point is a given distance from a plane.
pub const SLVS_C_PT_PLANE_DISTANCE: c_int = 100002;
/// A point is a given distance from a line.
pub const SLVS_C_PT_LINE_DISTANCE: c_int = 100003;
/// A point is a given distance from a face.
pub const SLVS_C_PT_FACE_DISTANCE: c_int = 100004;
/// A point lies in a plane.
pub const SLVS_C_PT_IN_PLANE: c_int = 100005;
/// A point lies on a line.
pub const SLVS_C_PT_ON_LINE: c_int = 100006;
/// A point lies on a face.
pub const SLVS_C_PT_ON_FACE: c_int = 100007;
/// Two lines have equal length.
pub const SLVS_C_EQUAL_LENGTH_LINES: c_int = 100008;
/// Two lines' lengths are in a given ratio.
pub const SLVS_C_LENGTH_RATIO: c_int = 100009;
/// A line's length equals a point-to-line distance.
pub const SLVS_C_EQ_LEN_PT_LINE_D: c_int = 100010;
/// Two point-to-line distances are equal.
pub const SLVS_C_EQ_PT_LN_DISTANCES: c_int = 100011;
/// Two pairs of lines meet at equal angles.
pub const SLVS_C_EQUAL_ANGLE: c_int = 100012;
/// A line's length equals an arc's length.
pub const SLVS_C_EQUAL_LINE_ARC_LEN: c_int = 100013;
/// Two points are symmetric about a plane.
pub const SLVS_C_SYMMETRIC: c_int = 100014;
/// Two points are symmetric about the workplane's horizontal axis.
pub const SLVS_C_SYMMETRIC_HORIZ: c_int = 100015;
/// Two points are symmetric about the workplane's vertical axis.
pub const SLVS_C_SYMMETRIC_VERT: c_int = 100016;
/// Two points are symmetric about a line.
pub const SLVS_C_SYMMETRIC_LINE: c_int = 100017;
/// A point sits at a line's midpoint.
pub const SLVS_C_AT_MIDPOINT: c_int = 100018;
/// A line is parallel to the workplane's horizontal axis.
pub const SLVS_C_HORIZONTAL: c_int = 100019;
/// A line is parallel to the workplane's vertical axis.
pub const SLVS_C_VERTICAL: c_int = 100020;
/// A circle or arc has a given diameter.
pub const SLVS_C_DIAMETER: c_int = 100021;
/// A point lies on a circle or arc.
pub const SLVS_C_PT_ON_CIRCLE: c_int = 100022;
/// Two normals have the same orientation.
pub const SLVS_C_SAME_ORIENTATION: c_int = 100023;
/// Two lines meet at a given angle, in degrees.
pub const SLVS_C_ANGLE: c_int = 100024;
/// Two lines are parallel.
pub const SLVS_C_PARALLEL: c_int = 100025;
/// Two lines are perpendicular.
pub const SLVS_C_PERPENDICULAR: c_int = 100026;
/// An arc and a line are tangent where they meet.
pub const SLVS_C_ARC_LINE_TANGENT: c_int = 100027;
/// A cubic and a line are tangent where they meet.
pub const SLVS_C_CUBIC_LINE_TANGENT: c_int = 100028;
/// Two circles or arcs have equal radius.
pub const SLVS_C_EQUAL_RADIUS: c_int = 100029;
/// Two points are a given distance apart, measured along a direction.
pub const SLVS_C_PROJ_PT_DISTANCE: c_int = 100030;
/// A point is pinned where it currently sits. A hard constraint, unlike the
/// solver's drag preference — see [`crate::System::drag`].
pub const SLVS_C_WHERE_DRAGGED: c_int = 100031;
/// Two curves are tangent where they meet.
pub const SLVS_C_CURVE_CURVE_TANGENT: c_int = 100032;
/// Two lines' lengths differ by a given amount.
pub const SLVS_C_LENGTH_DIFFERENCE: c_int = 100033;
/// Two arcs' lengths are in a given ratio.
pub const SLVS_C_ARC_ARC_LEN_RATIO: c_int = 100034;
/// An arc's and a line's lengths are in a given ratio.
pub const SLVS_C_ARC_LINE_LEN_RATIO: c_int = 100035;
/// Two arcs' lengths differ by a given amount.
pub const SLVS_C_ARC_ARC_DIFFERENCE: c_int = 100036;
/// An arc's and a line's lengths differ by a given amount.
pub const SLVS_C_ARC_LINE_DIFFERENCE: c_int = 100037;

// -- Result codes ----------------------------------------------------------

/// Every constraint was satisfied.
pub const SLVS_RESULT_OKAY: c_int = 0;
/// The constraints are mutually contradictory.
pub const SLVS_RESULT_INCONSISTENT: c_int = 1;
/// Newton's method did not converge.
pub const SLVS_RESULT_DIDNT_CONVERGE: c_int = 2;
/// The system exceeded the solver's hard variable limit.
pub const SLVS_RESULT_TOO_MANY_UNKNOWNS: c_int = 3;
/// Solved, but some constraints were redundant.
pub const SLVS_RESULT_REDUNDANT_OKAY: c_int = 4;

/// One scalar unknown. `val` is in/out: the solver writes the settled value back.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SlvsParam {
    /// This parameter's handle.
    pub h: SlvsHParam,
    /// The group the parameter belongs to.
    pub group: SlvsHGroup,
    /// The value: initial guess in, solution out.
    pub val: f64,
}

/// A geometric entity. Which of the fields are meaningful depends on `type_`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlvsEntity {
    /// This entity's handle.
    pub h: SlvsHEntity,
    /// The group the entity belongs to.
    pub group: SlvsHGroup,
    /// One of the `SLVS_E_*` constants.
    pub type_: c_int,
    /// The workplane the entity lives in, or [`SLVS_FREE_IN_3D`].
    pub wrkpl: SlvsHEntity,
    /// Defining points, by role — meaning varies by `type_`.
    pub point: [SlvsHEntity; 4],
    /// The defining normal, where one applies.
    pub normal: SlvsHEntity,
    /// The defining distance, where one applies.
    pub distance: SlvsHEntity,
    /// Parameters owned directly by the entity (a point's coordinates, say).
    pub param: [SlvsHParam; 4],
}

/// A constraint. Which operand fields are meaningful depends on `type_`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SlvsConstraint {
    /// This constraint's handle.
    pub h: SlvsHConstraint,
    /// The group the constraint belongs to.
    pub group: SlvsHGroup,
    /// One of the `SLVS_C_*` constants.
    pub type_: c_int,
    /// The workplane the constraint is measured in, or [`SLVS_FREE_IN_3D`].
    pub wrkpl: SlvsHEntity,
    /// The constraint's value — a distance, angle or ratio.
    pub val_a: f64,
    /// First point operand.
    pub pt_a: SlvsHEntity,
    /// Second point operand.
    pub pt_b: SlvsHEntity,
    /// First entity operand.
    pub entity_a: SlvsHEntity,
    /// Second entity operand.
    pub entity_b: SlvsHEntity,
    /// Third entity operand.
    pub entity_c: SlvsHEntity,
    /// Fourth entity operand.
    pub entity_d: SlvsHEntity,
    /// Selects between the two solutions a constraint may admit.
    pub other: c_int,
    /// A second such selector, for constraints that need one.
    pub other2: c_int,
}

/// The whole problem handed to the solver in one call.
///
/// Every array is owned by the caller. [`crate::System`] holds them in `Vec`s
/// and fills this in only for the duration of the call.
#[repr(C)]
#[derive(Debug)]
pub struct SlvsSystem {
    /// Parameters. In/out: solved values are written back here.
    pub param: *mut SlvsParam,
    /// Length of `param`.
    pub params: c_int,
    /// Entities.
    pub entity: *mut SlvsEntity,
    /// Length of `entity`.
    pub entities: c_int,
    /// Constraints.
    pub constraint: *mut SlvsConstraint,
    /// Length of `constraint`.
    pub constraints: c_int,
    /// Parameters the user is dragging: the solver favours leaving these alone.
    pub dragged: *mut SlvsHParam,
    /// Length of `dragged`.
    pub ndragged: c_int,
    /// Whether to spend the extra time working out which constraints failed.
    pub calculate_faileds: c_int,
    /// Caller-allocated output buffer for failing constraint handles.
    pub failed: *mut SlvsHConstraint,
    /// In: the capacity of `failed`. Out: how many were written.
    pub faileds: c_int,
    /// Out: remaining unconstrained degrees of freedom.
    pub dof: c_int,
    /// Out: one of the `SLVS_RESULT_*` constants.
    pub result: c_int,
}

unsafe extern "C" {
    /// Solve group `hg` of `sys` in place.
    pub fn Slvs_Solve(sys: *mut SlvsSystem, hg: SlvsHGroup);

    /// Build a unit quaternion from two basis vectors.
    pub fn Slvs_MakeQuaternion(
        ux: f64,
        uy: f64,
        uz: f64,
        vx: f64,
        vy: f64,
        vz: f64,
        qw: *mut f64,
        qx: *mut f64,
        qy: *mut f64,
        qz: *mut f64,
    );

    /// Recover the U basis vector of the frame a quaternion describes.
    pub fn Slvs_QuaternionU(
        qw: f64,
        qx: f64,
        qy: f64,
        qz: f64,
        x: *mut f64,
        y: *mut f64,
        z: *mut f64,
    );

    /// Recover the V basis vector of the frame a quaternion describes.
    pub fn Slvs_QuaternionV(
        qw: f64,
        qx: f64,
        qy: f64,
        qz: f64,
        x: *mut f64,
        y: *mut f64,
        z: *mut f64,
    );

    /// Recover the normal of the frame a quaternion describes.
    pub fn Slvs_QuaternionN(
        qw: f64,
        qx: f64,
        qy: f64,
        qz: f64,
        x: *mut f64,
        y: *mut f64,
        z: *mut f64,
    );
}
