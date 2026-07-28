//! Quasi-Newton descent, driven by the [`argmin`] optimization crate.
//!
//! The division of labour is the point of this module: **argmin owns the
//! algorithm** (the L-BFGS two-loop recursion, the curvature history, the
//! More–Thuente line search and its Wolfe conditions), and this crate owns
//! only the problem — the cost and the gradient, from
//! [`PackEnergy`]. Hand-rolling a line search
//! that reliably satisfies the strong Wolfe conditions is exactly the kind
//! of numerical work that is easy to get subtly, silently wrong, and there
//! is nothing packing-specific about it.
//!
//! # Why More–Thuente and not Hager–Zhang
//!
//! Both are in argmin and Hager–Zhang is the more modern choice, but its
//! bracketing update is an **unbounded loop** that exits only when the
//! directional derivative changes sign. On a packing energy — which is only
//! piecewise smooth, since a boundary clamp and a rotation snap both
//! introduce kinks — that condition can simply never be met, and the solver
//! hangs instead of returning an error. More–Thuente is implemented as an
//! argmin `Solver` in its own right, so its iterations are bounded by
//! `max_iters` and the worst case is a failed step, not a frozen editor.
//!
//! # Stepping it one iteration at a time
//!
//! argmin's usual entry point is `Executor::run()`, which iterates to
//! completion in one blocking call. That is unusable here: the editor draws
//! the search as it converges, so a solver has to yield after every
//! iteration. So this drives argmin's [`Solver`](argmin::core::Solver) trait
//! directly — `init` once, then `next_iter` per step — which is the
//! documented manual path and keeps the curvature history alive across
//! frames. Restarting an executor each frame would throw that history away
//! and reduce L-BFGS to plain gradient descent.
//!
//! # Where it is strong and where it is not
//!
//! Second-order information makes this by far the fastest way to *polish* an
//! arrangement that is already roughly right — it closes the last few
//! percent of fill in tens of iterations where relaxation takes hundreds.
//! But it strictly descends, so it cannot leave the basin it starts in. On a
//! scattered pile it will happily converge to the scattered pile's nearest
//! local minimum, which is why [`SolverKind::wants_warm_start`](crate::problem::SolverKind::wants_warm_start) is true for
//! it and the run seeds it from a shelf packing by default.

// argmin's own `terminate_internal` is deliberately *not* used: it unwraps
// the state's gradient and panics when there is none, and stopping rules are
// `PackRun`'s job for every solver in this crate, not each solver's.
use argmin::core::{
    CostFunction, Executor, Gradient, IterState, Problem, Solver as ArgminSolver, State,
};
use argmin::solver::linesearch::MoreThuenteLineSearch;
use argmin::solver::quasinewton::LBFGS;
use std::cell::RefCell;

use crate::gradient::PackEnergy;
use crate::objective::Scratch;
use crate::problem::{Layout, PackProblem};
use crate::solver::Solver;

/// Largest parameter movement (metres/radians) still counted as "stationary".
const STATIONARY: f64 = 1e-9;

/// L∞ distance between two parameter vectors.
fn moved(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0_f64, f64::max)
}

/// How many past gradient pairs L-BFGS keeps. Seven is the textbook default
/// and is plenty: the packing Hessian is dominated by local pair contacts,
/// so old curvature stops being informative quickly.
const MEMORY: usize = 7;

/// The argmin problem adapter.
///
/// argmin's `CostFunction`/`Gradient` take `&self`, but evaluating either
/// needs the placement scratch buffers — reallocating them per evaluation
/// would dominate the cost of the whole solver. `RefCell` gives interior
/// mutability for that cache; argmin never calls these concurrently (it is
/// single-threaded unless the `rayon` feature is on, which this workspace
/// does not enable).
struct Energy<'a> {
    energy: PackEnergy<'a>,
    scratch: RefCell<Scratch>,
}

impl CostFunction for Energy<'_> {
    type Param = Vec<f64>;
    type Output = f64;

    fn cost(&self, param: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
        Ok(self.energy.cost(param, &mut self.scratch.borrow_mut()))
    }
}

impl Gradient for Energy<'_> {
    type Param = Vec<f64>;
    type Gradient = Vec<f64>;

    fn gradient(&self, param: &Self::Param) -> Result<Self::Gradient, argmin::core::Error> {
        let (_, grad) = self
            .energy
            .cost_and_gradient(param, &mut self.scratch.borrow_mut());
        Ok(grad)
    }
}

type DescentState = IterState<Vec<f64>, Vec<f64>, (), (), (), f64>;
type Lbfgs = LBFGS<MoreThuenteLineSearch<Vec<f64>, Vec<f64>, f64>, Vec<f64>, Vec<f64>, f64>;

/// L-BFGS over the packing energy.
pub struct DescentSolver {
    layout: Layout,
    /// `None` once argmin has reported it cannot make further progress; the
    /// run driver then sees a stationary layout and converges normally.
    inner: Option<(Lbfgs, DescentState)>,
    stalled: bool,
}

impl DescentSolver {
    /// Starts from the problem's current arrangement.
    pub fn new(problem: &PackProblem) -> Self {
        Self {
            layout: Layout::from_starts(&problem.items),
            inner: None,
            stalled: false,
        }
    }

    /// Builds the argmin solver and its initial state.
    fn initialize(&mut self, problem: &PackProblem) {
        let energy = PackEnergy::new(problem);
        if energy.dim() == 0 {
            self.stalled = true;
            return;
        }
        let params = energy.to_params(&self.layout);
        let scratch = Scratch::new(problem.len());
        let mut adapter = Problem::new(Energy {
            energy,
            scratch: RefCell::new(scratch),
        });
        let linesearch = MoreThuenteLineSearch::new();
        let mut solver: Lbfgs = LBFGS::new(linesearch, MEMORY);
        let state = IterState::new()
            .param(params)
            .max_iters(u64::from(problem.config.max_iterations));
        match solver.init(&mut adapter, state) {
            Ok((state, _)) => self.inner = Some((solver, state)),
            Err(_) => self.stalled = true,
        }
    }
}

impl Solver for DescentSolver {
    fn name(&self) -> &'static str {
        "L-BFGS (argmin)"
    }

    fn layout(&self) -> &Layout {
        &self.layout
    }

    fn seed(&mut self, layout: Layout) {
        self.layout = layout;
    }

    fn is_one_shot(&self) -> bool {
        // Not one-shot, but once argmin gives up there is nothing more to
        // extract and further iterations would spin.
        self.stalled
    }

    fn step(&mut self, problem: &PackProblem, _scratch: &mut Scratch) {
        if self.stalled {
            return;
        }
        if self.inner.is_none() {
            self.initialize(problem);
        }
        let Some((solver, state)) = self.inner.take() else {
            return;
        };

        let energy = PackEnergy::new(problem);
        let mut adapter = Problem::new(Energy {
            energy,
            scratch: RefCell::new(Scratch::new(problem.len())),
        });
        let mut solver = solver;
        let previous = state.get_param().cloned();
        match solver.next_iter(&mut adapter, state) {
            Ok((next, _)) => {
                if let Some(param) = next.get_param() {
                    self.layout = PackEnergy::new(problem).to_layout(param);
                    // A step that moved nothing means the line search could
                    // not find an acceptable point: the iterate is stationary
                    // and further iterations would re-derive the same
                    // direction forever.
                    if previous.is_some_and(|before| moved(&before, param) < STATIONARY) {
                        self.stalled = true;
                    }
                }
                if next.get_iter() >= next.get_max_iters() {
                    self.stalled = true;
                }
                self.inner = Some((solver, next));
            }
            // A line search failure is a normal outcome near a minimum, not
            // an error worth surfacing — the run simply has nothing left to
            // extract.
            Err(_) => self.stalled = true,
        }
    }
}

/// Runs L-BFGS to completion in one call, outside the per-frame stepping.
///
/// Used by the warm-start polish and available to callers (tests, scripts)
/// that want an answer rather than an animation.
pub fn solve_to_completion(problem: &PackProblem, layout: &Layout, max_iters: u64) -> Layout {
    let energy = PackEnergy::new(problem);
    if energy.dim() == 0 {
        return layout.clone();
    }
    let params = energy.to_params(layout);
    let adapter = Energy {
        energy: PackEnergy::new(problem),
        scratch: RefCell::new(Scratch::new(problem.len())),
    };
    let linesearch = MoreThuenteLineSearch::new();
    let solver: Lbfgs = LBFGS::new(linesearch, MEMORY);
    let result = Executor::new(adapter, solver)
        .configure(|s| s.param(params).max_iters(max_iters))
        .run();
    match result {
        Ok(res) => res
            .state()
            .get_best_param()
            .map_or_else(|| layout.clone(), |p| PackEnergy::new(problem).to_layout(p)),
        Err(_) => layout.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::metrics;
    use crate::problem::{PackConfig, PackItem, RotationMode, SolverKind};
    use crate::solver::PackRun;
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

    fn config() -> PackConfig {
        PackConfig {
            solver: SolverKind::Descent,
            clearance: 0.0,
            rotation: RotationMode::Fixed,
            max_iterations: 300,
            patience: 60,
            warm_start: true,
            ..Default::default()
        }
    }

    #[test]
    fn descent_reaches_a_legal_and_tighter_arrangement() {
        let items = (0..6)
            .map(|i| square(Vec2::new(i as f32 * 3.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        let report = run.report();
        assert!(report.best.is_feasible(), "overlap {}", report.best.overlap);
        assert!(
            report.shrinkage() > 0.5,
            "expected real progress, got {:.3}",
            report.shrinkage()
        );
    }

    #[test]
    fn descent_terminates_rather_than_spinning() {
        let items = (0..5)
            .map(|i| square(Vec2::new(i as f32 * 2.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        assert!(run.solve().is_done());
    }

    #[test]
    fn a_pinned_only_problem_does_not_panic() {
        let mut a = square(Vec2::ZERO, 0.5);
        let mut b = square(Vec2::new(3.0, 0.0), 0.5);
        a.pinned = true;
        b.pinned = true;
        let mut run = PackRun::new(PackProblem::new(vec![a, b], config()));
        run.solve();
        // Nothing movable: the run reports Empty rather than iterating.
        assert!(run.status().is_done());
    }

    #[test]
    fn solving_to_completion_never_returns_a_worse_layout() {
        let items: Vec<PackItem> = (0..5)
            .map(|i| square(Vec2::new(i as f32 * 2.5, 0.0), 0.5))
            .collect();
        let problem = PackProblem::new(items, config());
        let start = Layout::from_starts(&problem.items);
        let mut scratch = Scratch::new(problem.len());
        let before = metrics(&problem, &start, &mut scratch).objective;
        let solved = solve_to_completion(&problem, &start, 200);
        let after = metrics(&problem, &solved, &mut scratch).objective;
        assert!(after <= before + 1e-4, "{before} -> {after}");
    }
}
