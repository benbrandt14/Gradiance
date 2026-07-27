//! Layout optimization — the arrangement solver family.
//!
//! Given a set of shapes and a rulebook, find where to put them so they fit
//! into the smallest area without colliding. The first (and so far only)
//! problem class is **close packing** of a selection, but the crate is
//! deliberately shaped as a family: a [`problem`] description, an
//! [`objective`] to minimize, and a [`Solver`](solver::Solver) trait with
//! several interchangeable strategies behind it.
//!
//! # Why this is not the physics engine
//!
//! It would be tempting to pack bodies by turning up gravity and letting
//! avian settle them. That is a different thing and a worse one: a physics
//! step is a *simulation* of an arrangement forming, so it inherits masses,
//! restitution, friction, sleeping, and time step — none of which the user
//! is trying to specify — and it only ever finds the arrangement its
//! dynamics happen to fall into. There is no objective function, so there is
//! nothing to converge *to*, no way to ask for "smallest bounding box" or
//! "fit this aspect ratio", no reproducibility, and no way to escape a bad
//! local minimum.
//!
//! Everything here is instead a **geometric search over poses**: no mass, no
//! time, no contacts, no avian. That is what makes results reproducible from
//! a seed, comparable through one scalar, and interruptible mid-search — the
//! three properties the editor's preview and the eventual scripting bindings
//! both need.
//!
//! # Layering
//!
//! Pure math, in the same sense as `gradiance-geometry`: no systems, no
//! queries, no `World`. The only Bevy surface is the `Resource`/`Reflect`
//! derives on [`PackConfig`] (editor configuration, addressable by the
//! scripting registry). The ECS driver that runs a solve across frames and
//! turns the answer into pose commands lives in `gradiance-interaction`; the
//! command discipline is untouched — a finished pack emits exactly one
//! transform intent like any other gesture.
//!
//! # Shape of a solve
//!
//! ```
//! use bevy::math::Vec2;
//! use gradiance_optimize::{PackConfig, PackItem, PackProblem, PackRun, SolverKind};
//!
//! // Two unit squares, sitting a long way apart.
//! let square = |cx: f32| {
//!     PackItem::from_world_outline(
//!         &[
//!             Vec2::new(cx - 0.5, -0.5),
//!             Vec2::new(cx + 0.5, -0.5),
//!             Vec2::new(cx + 0.5, 0.5),
//!             Vec2::new(cx - 0.5, 0.5),
//!         ],
//!         0.0,   // world angle
//!         0b1,   // collision-layer bits from the depth band
//!         false, // not pinned
//!     )
//! };
//! let problem = PackProblem::new(
//!     vec![square(0.0), square(20.0)],
//!     PackConfig {
//!         solver: SolverKind::Shelf,
//!         ..Default::default()
//!     },
//! );
//!
//! let mut run = PackRun::new(problem);
//! run.solve(); // or `advance(n)` once per frame, to animate it
//!
//! let report = run.report();
//! assert!(report.status.is_done());
//! assert!(report.best.is_feasible(), "no overlaps in the answer");
//! assert!(report.shrinkage() > 0.8, "much smaller than it started");
//! ```

pub mod gradient;
pub mod hull;
pub mod objective;
pub mod problem;
pub mod rng;
pub mod sat;
pub mod solver;
pub mod solvers;

pub use objective::{Metrics, Scratch, metrics};
pub use problem::{
    AnnealParams, Boundary, EdgeAlignment, LayerRule, Layout, Objective, ObjectiveWeights,
    PackConfig, PackItem, PackProblem, RelaxParams, RotationMode, ShelfOrder, ShelfParams,
    SolverKind,
};
pub use solver::{PackReport, PackRun, RunStatus, Solver};
