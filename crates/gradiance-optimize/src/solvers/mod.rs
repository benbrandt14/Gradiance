//! The solver implementations.
//!
//! Adding a strategy is: a module here implementing
//! [`Solver`](crate::solver::Solver), a variant on
//! [`SolverKind`](crate::problem::SolverKind), and one row in [`build`].
//! Stopping rules, restarts, and best-so-far tracking are *not* a solver's
//! business — [`PackRun`](crate::solver::PackRun) owns those for everyone.

pub mod anneal;
pub mod relax;
pub mod shelf;

use crate::problem::{PackProblem, SolverKind};
use crate::solver::Solver;

/// Instantiates the configured solver, seeded for reproducibility.
pub fn build(problem: &PackProblem, seed: u64) -> Box<dyn Solver> {
    match problem.config.solver {
        SolverKind::Shelf => Box::new(shelf::ShelfSolver::new(problem)),
        SolverKind::Relax => Box::new(relax::RelaxSolver::new(problem, seed)),
        SolverKind::Anneal => Box::new(anneal::AnnealSolver::new(problem, seed)),
    }
}
