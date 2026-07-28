//! The solver interface and the run driver every solver shares.
//!
//! A [`Solver`] does one job: given the problem, mutate its working
//! [`Layout`] by one iteration. Everything *around* the iteration —
//! convergence bookkeeping, keeping the best-so-far, restarting from a new
//! seed, deciding when to stop — lives once in [`PackRun`]. That is why
//! adding a fourth strategy is a single file implementing three methods and
//! one row in [`crate::solvers::build`], with no new stopping logic to get
//! subtly wrong.
//!
//! The run is **stepped, not blocking**: the editor calls
//! [`PackRun::advance`] with a per-frame budget so the arrangement can be
//! drawn as it converges. Nothing here spawns a thread or owns a clock.

use crate::objective::{Metrics, Scratch, metrics};
use crate::problem::{Layout, PackProblem};

/// Where a run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunStatus {
    /// Still iterating.
    Running,
    /// Stopped improving by more than the tolerance for `patience`
    /// iterations — the normal, successful ending.
    Converged,
    /// Hit the iteration ceiling while still improving. The result is
    /// usable; raising `max_iterations` would sharpen it.
    Exhausted,
    /// Nothing to solve (fewer than two movable items).
    Empty,
}

impl RunStatus {
    /// Whether the run has stopped.
    pub fn is_done(self) -> bool {
        !matches!(self, Self::Running)
    }

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Converged => "converged",
            Self::Exhausted => "iteration limit",
            Self::Empty => "nothing to pack",
        }
    }
}

/// One packing strategy.
///
/// Implementors own their working [`Layout`] and any per-strategy state
/// (temperature, velocities, placement cursor). They must **not** implement
/// their own stopping rule: return normally and let [`PackRun`] decide, so
/// every solver honors the same user-facing tolerance and iteration budget.
pub trait Solver: Send + Sync + 'static {
    /// Name shown in the run readout.
    fn name(&self) -> &'static str;

    /// Advances the working layout by one iteration.
    ///
    /// `scratch` is a shared placement cache; it holds the working layout's
    /// placed hulls on entry and may be refreshed freely.
    fn step(&mut self, problem: &PackProblem, scratch: &mut Scratch);

    /// The current working layout.
    fn layout(&self) -> &Layout;

    /// Replaces the working layout — the warm-start hook.
    ///
    /// Default is to ignore it, which is right for the constructive
    /// strategies: a shelf packing does not care where anything started, so
    /// seeding one would be a no-op with extra steps.
    fn seed(&mut self, _layout: Layout) {}

    /// True for one-shot constructive strategies, which are complete after
    /// a single step and should not be iterated further.
    fn is_one_shot(&self) -> bool {
        false
    }
}

/// A live packing run: problem, solver, best-so-far, and stopping state.
pub struct PackRun {
    problem: PackProblem,
    solver: Box<dyn Solver>,
    scratch: Scratch,
    start: Metrics,
    current: Metrics,
    best_layout: Layout,
    best: Metrics,
    iteration: u32,
    total_iterations: u32,
    since_improvement: u32,
    status: RunStatus,
}

impl std::fmt::Debug for PackRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackRun")
            .field("solver", &self.solver.name())
            .field("status", &self.status)
            .field("iteration", &self.iteration)
            .field("best", &self.best.extent)
            .finish_non_exhaustive()
    }
}

impl PackRun {
    /// Starts a run over `problem`.
    ///
    /// A problem with fewer than two movable items has nothing to arrange
    /// and comes back already [`RunStatus::Empty`], so callers never have to
    /// special-case a one-body selection.
    pub fn new(problem: PackProblem) -> Self {
        let mut scratch = Scratch::new(problem.items.len());
        let start_layout = Layout::from_starts(&problem.items);
        let start = metrics(&problem, &start_layout, &mut scratch);
        let movable = (0..problem.len()).filter(|i| problem.movable(*i)).count();
        let solver = crate::solvers::build(&problem);
        let status = if problem.len() < 2 || movable == 0 {
            RunStatus::Empty
        } else {
            RunStatus::Running
        };
        Self {
            solver,
            scratch,
            start,
            current: start,
            best_layout: start_layout,
            best: start,
            iteration: 0,
            total_iterations: 0,
            since_improvement: 0,
            status,
            problem,
        }
    }

    /// The problem being solved.
    pub fn problem(&self) -> &PackProblem {
        &self.problem
    }

    /// The layout the solver is working on right now — what the preview
    /// ghost draws, so the user sees the search rather than only its answer.
    pub fn working_layout(&self) -> &Layout {
        self.solver.layout()
    }

    /// The best layout found so far — what gets applied.
    pub fn best_layout(&self) -> &Layout {
        &self.best_layout
    }

    /// Current run status.
    pub fn status(&self) -> RunStatus {
        self.status
    }

    /// A snapshot for the UI.
    pub fn report(&self) -> PackReport {
        PackReport {
            solver: self.solver.name(),
            status: self.status,
            iteration: self.iteration,
            total_iterations: self.total_iterations,
            max_iterations: self.problem.config.max_iterations,
            start: self.start,
            current: self.current,
            best: self.best,
        }
    }

    /// Runs a single iteration. No-op once the run is done.
    pub fn step(&mut self) -> RunStatus {
        if self.status.is_done() {
            return self.status;
        }
        self.solver.step(&self.problem, &mut self.scratch);
        self.iteration += 1;
        self.total_iterations += 1;

        self.current = metrics(&self.problem, self.solver.layout(), &mut self.scratch);
        let improved_by = relative_gain(self.best, self.current);
        if is_better(&self.current, &self.best) {
            self.best = self.current;
            self.best_layout = self.solver.layout().clone();
        }
        if improved_by > self.problem.config.tolerance {
            self.since_improvement = 0;
        } else {
            self.since_improvement += 1;
        }

        // A constructive solver is finished the moment it has placed
        // everything; iterating it again would just redo the same placement.
        let solver_done = self.solver.is_one_shot();
        if solver_done || self.since_improvement >= self.problem.config.patience {
            self.finish_attempt(RunStatus::Converged);
        } else if self.iteration >= self.problem.config.max_iterations {
            self.finish_attempt(RunStatus::Exhausted);
        }
        self.status
    }

    /// Ends the run.
    ///
    /// This used to start the next restart. Restarts only ever bought
    /// anything for a stochastic search — re-running a deterministic solver
    /// reproduces the same layout — and every solver here is now
    /// deterministic, so the branch (and the seed that fed it) is gone.
    fn finish_attempt(&mut self, outcome: RunStatus) {
        self.status = outcome;
    }

    /// Runs up to `budget` iterations, stopping early if the run finishes.
    pub fn advance(&mut self, budget: u32) -> RunStatus {
        for _ in 0..budget {
            if self.step().is_done() {
                break;
            }
        }
        self.status
    }

    /// Runs to completion, ignoring the per-frame budget. Bounded by
    /// `max_iterations`, so it always terminates.
    pub fn solve(&mut self) -> RunStatus {
        self.advance(self.problem.config.max_iterations.saturating_add(1))
    }
}

/// Whether `candidate` beats `incumbent`.
///
/// Feasibility is lexicographically first: a legal arrangement always beats
/// an illegal one, however much smaller the illegal one is. Without that a
/// deep overlap could win on extent alone and the run would "succeed" with
/// bodies inside each other.
fn is_better(candidate: &Metrics, incumbent: &Metrics) -> bool {
    match (candidate.is_feasible(), incumbent.is_feasible()) {
        (true, false) => true,
        (false, true) => false,
        _ => candidate.objective < incumbent.objective,
    }
}

/// Relative improvement of `candidate` over `incumbent`, or 0 when it is
/// not an improvement.
fn relative_gain(incumbent: Metrics, candidate: Metrics) -> f32 {
    if !is_better(&candidate, &incumbent) {
        return 0.0;
    }
    let denom = incumbent.objective.abs().max(1e-9);
    ((incumbent.objective - candidate.objective) / denom).max(0.0)
}

/// A UI-facing snapshot of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackReport {
    /// Name of the running solver.
    pub solver: &'static str,
    /// Where the run stands.
    pub status: RunStatus,
    /// Iterations in the current attempt.
    pub iteration: u32,
    /// Iterations across all attempts.
    pub total_iterations: u32,
    /// Configured iteration ceiling per attempt.
    pub max_iterations: u32,
    /// Score of the untouched selection.
    pub start: Metrics,
    /// Score of the layout being drawn right now.
    pub current: Metrics,
    /// Score of the layout that would be applied.
    pub best: Metrics,
}

impl PackReport {
    /// Fractional reduction in extent versus the starting arrangement
    /// (0.25 = a quarter smaller). Negative if the best is somehow worse.
    pub fn shrinkage(&self) -> f32 {
        if self.start.extent.abs() < 1e-9 {
            return 0.0;
        }
        (self.start.extent - self.best.extent) / self.start.extent
    }

    /// Progress through the iteration budget, in `0..=1`.
    pub fn progress(&self) -> f32 {
        if self.status.is_done() {
            return 1.0;
        }
        (self.iteration as f32 / self.max_iterations.max(1) as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::{PackConfig, PackItem, SolverKind};
    use bevy::math::Vec2;

    fn square(center: Vec2, half: f32) -> PackItem {
        PackItem::from_world_outline(
            &[
                center + Vec2::new(-half, -half),
                center + Vec2::new(half, -half),
                center + Vec2::new(half, half),
                center + Vec2::new(-half, half),
            ],
            0.0,
            1,
            false,
        )
    }

    #[test]
    fn a_single_item_run_is_empty_rather_than_spinning() {
        let run = PackRun::new(PackProblem::new(
            vec![square(Vec2::ZERO, 0.5)],
            PackConfig::default(),
        ));
        assert_eq!(run.status(), RunStatus::Empty);
    }

    #[test]
    fn an_all_pinned_run_is_empty() {
        let mut a = square(Vec2::ZERO, 0.5);
        let mut b = square(Vec2::new(4.0, 0.0), 0.5);
        a.pinned = true;
        b.pinned = true;
        let run = PackRun::new(PackProblem::new(vec![a, b], PackConfig::default()));
        assert_eq!(run.status(), RunStatus::Empty);
    }

    #[test]
    fn stepping_a_finished_run_is_a_no_op() {
        let mut run = PackRun::new(PackProblem::new(
            vec![square(Vec2::ZERO, 0.5)],
            PackConfig::default(),
        ));
        assert_eq!(run.step(), RunStatus::Empty);
        assert_eq!(run.report().iteration, 0);
    }

    #[test]
    fn feasibility_outranks_a_smaller_but_illegal_layout() {
        let legal = Metrics {
            objective: 100.0,
            extent: 100.0,
            bounds: (Vec2::ZERO, Vec2::ONE),
            overlap: 0.0,
            violations: 0,
            fill: 1.0,
            hull_fill: 1.0,
            min_gap: 0.0,
            mean_gap: 0.0,
            contact: 0.0,
            alignment: 1.0,
            boundary_error: 0.0,
        };
        let tiny_but_illegal = Metrics {
            objective: 1.0,
            extent: 1.0,
            overlap: 0.5,
            violations: 3,
            ..legal
        };
        assert!(is_better(&legal, &tiny_but_illegal));
        assert!(!is_better(&tiny_but_illegal, &legal));
    }

    #[test]
    fn a_run_always_terminates_within_its_budget() {
        let items = (0..6)
            .map(|i| square(Vec2::new(i as f32 * 0.3, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                solver: SolverKind::Descent,
                max_iterations: 50,
                patience: 50,
                ..Default::default()
            },
        ));
        assert!(run.solve().is_done());
        assert!(run.report().total_iterations <= 51);
    }
}
