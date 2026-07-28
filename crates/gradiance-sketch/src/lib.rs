//! Constrained 2D sketching.
//!
//! A sketch is geometry plus the *relationships* the author asked for, settled
//! by SolveSpace's geometric constraint solver. It is an authoring-time
//! subsystem and is deliberately isolated from the simulation:
//!
//! - this crate depends on `core` and `geometry` only — **no physics, no
//!   avian**, which the workspace DAG turns into a compile error rather than a
//!   review note;
//! - the solver produces geometry. It never writes a `Transform`, a velocity,
//!   a joint, or any physics component;
//! - sketch mode runs with the simulation paused, so solving and stepping never
//!   interleave.
//!
//! [`doc`] is the authored document, [`solve`](mod@solve) is the SolveSpace bridge, and
//! [`lower`] turns a settled sketch into a [`ShapeDef`](gradiance_geometry::shape::ShapeDef).
//! Only `lower` is 2D-specific: `doc` and `solve` speak SolveSpace's native 3D
//! workplane, so a future 3D construction plane is a new workplane rather than
//! a redesign.

pub mod annotate;
pub mod doc;
pub mod edit;
pub mod lower;
pub mod ops;
pub mod pick;
pub mod solve;

pub use doc::{SketchConstraint, SketchDoc, SketchEntity, SketchId, SketchPoint};
pub use edit::{ConstraintKind, EditError, SketchSelection};
pub use ops::OpError;
pub use pick::{PickHit, SketchTarget, SnapKind};
pub use solve::{SketchError, SolveOutcome, SolveStatus, solve};
