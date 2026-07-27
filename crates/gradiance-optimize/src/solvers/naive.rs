//! The naive baseline — deliberately the *bad* way to pack.
//!
//! Every iteration: pull every body toward the arrangement centroid, push
//! overlapping pairs apart, repeat. No objective function, no convergence
//! criterion beyond "nothing is moving much", no notion of what a good
//! arrangement would be. This is what turning up gravity or adding an
//! attractor in the physics engine amounts to, and it is the most obvious
//! thing to reach for.
//!
//! It is here to be **measured against**, and it earns its place by being
//! the honest version of the obvious approach — a straw man would prove
//! nothing. It gets the same separation quality, the same clearance
//! handling, the same boundary clamping, and the same per-iteration step
//! limit as [`RelaxSolver`](super::relax::RelaxSolver). The single
//! difference is the one that matters: attraction and separation act
//! together on every iteration, so the arrangement settles at the point
//! where the two forces *balance* rather than at the point where the
//! objective is smallest.
//!
//! That balance is exactly the failure mode. The equilibrium gap between two
//! bodies is set by the ratio of the attraction gain to the separation gain
//! — a tuning constant — instead of by anything about the packing. Turn
//! attraction up and bodies interpenetrate; turn it down and they stop early
//! with visible slack. There is no setting that produces a *tight* packing,
//! because tightness was never what the method was computing.
//!
//! `tests/quality.rs` asserts the real solvers beat this one on fill ratio.

use bevy::math::Vec2;
use gradiance_core::units::PosRot;

use crate::objective::{Scratch, clamp_to_boundary};
use crate::problem::{Layout, PackProblem};
use crate::sat::penetration;
use crate::solver::Solver;

/// Attraction-plus-separation with no objective.
pub struct NaiveSolver {
    layout: Layout,
    correction: Vec<Vec2>,
    target: Vec2,
}

impl NaiveSolver {
    /// Starts from the problem's current arrangement.
    pub fn new(problem: &PackProblem) -> Self {
        Self {
            layout: Layout::from_starts(&problem.items),
            correction: vec![Vec2::ZERO; problem.items.len()],
            target: problem.start_center(),
        }
    }
}

impl Solver for NaiveSolver {
    fn name(&self) -> &'static str {
        "naive attraction"
    }

    fn layout(&self) -> &Layout {
        &self.layout
    }

    fn seed(&mut self, layout: Layout) {
        self.layout = layout;
    }

    fn step(&mut self, problem: &PackProblem, scratch: &mut Scratch) {
        let n = problem.items.len();
        let cfg = &problem.config;
        let params = cfg.relax;
        scratch.refresh(problem, &self.layout);

        self.correction.clear();
        self.correction.resize(n, Vec2::ZERO);

        for i in 0..n {
            for j in (i + 1)..n {
                if !problem.pair_collides(i, j) {
                    continue;
                }
                let (movable_i, movable_j) = (problem.movable(i), problem.movable(j));
                if !movable_i && !movable_j {
                    continue;
                }
                let (Some(a), Some(b)) = (problem.items.get(i), problem.items.get(j)) else {
                    continue;
                };
                let reach = a.radius + b.radius + cfg.clearance;
                if scratch.center(i).distance_squared(scratch.center(j)) > reach * reach {
                    continue;
                }
                let Some(mtv) = penetration(scratch.placed(i), scratch.placed(j), cfg.clearance)
                else {
                    continue;
                };
                let push = mtv.axis * mtv.depth * params.separation_gain;
                match (movable_i, movable_j) {
                    (true, true) => {
                        self.correction[i] -= push * 0.5;
                        self.correction[j] += push * 0.5;
                    }
                    (true, false) => self.correction[i] -= push,
                    (false, true) => self.correction[j] += push,
                    (false, false) => {}
                }
            }
        }

        // The defining line: attraction on *every* iteration, at the same
        // time as separation, so the two settle into a force balance.
        for i in 0..n {
            if !problem.movable(i) {
                continue;
            }
            let pos = self.layout.poses[i].pos;
            self.correction[i] += (self.target - pos) * params.attraction;
            self.correction[i] += cfg.gravity_bias * params.attraction;
        }

        for i in 0..n {
            if !problem.movable(i) {
                continue;
            }
            let Some(item) = problem.items.get(i) else {
                continue;
            };
            let mut v = self.correction[i];
            if v.length() > cfg.max_step {
                v = v.normalize_or_zero() * cfg.max_step;
            }
            let mut pose = PosRot {
                pos: self.layout.poses[i].pos + v,
                rot: self.layout.poses[i].rot,
            };
            pose.pos = clamp_to_boundary(problem, pose, item.radius);
            self.layout.poses[i] = pose;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::{PackConfig, PackItem, RotationMode, SolverKind};
    use crate::solver::PackRun;

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
            solver: SolverKind::Naive,
            clearance: 0.0,
            rotation: RotationMode::Fixed,
            max_iterations: 3000,
            patience: 300,
            warm_start: false,
            ..Default::default()
        }
    }

    #[test]
    fn the_baseline_does_gather_a_scattered_set() {
        // It has to actually work, or beating it would prove nothing.
        let items = (0..6)
            .map(|i| square(Vec2::new(i as f32 * 4.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        assert!(
            run.report().shrinkage() > 0.4,
            "the baseline must be a real method, not a straw man: {:.3}",
            run.report().shrinkage()
        );
    }

    #[test]
    fn the_baseline_settles_at_a_force_balance_not_a_packing() {
        // Its equilibrium is set by the gain ratio, so cranking attraction
        // drives bodies *into* each other rather than packing them tighter.
        // This is the property that makes it unsuitable, pinned down.
        let items: Vec<PackItem> = (0..5)
            .map(|i| square(Vec2::new(i as f32 * 2.0, 0.0), 0.5))
            .collect();
        let greedy = PackConfig {
            relax: crate::problem::RelaxParams {
                attraction: 0.4,
                ..Default::default()
            },
            ..config()
        };
        let mut run = PackRun::new(PackProblem::new(items, greedy));
        run.solve();
        // The final *working* layout is what a physics settle would hand you
        // (no best-so-far bookkeeping exists in that world).
        let mut scratch = Scratch::new(run.problem().len());
        let live = crate::objective::metrics(run.problem(), run.working_layout(), &mut scratch);
        assert!(
            !live.is_feasible(),
            "a hard pull should leave residual interpenetration, overlap was {:.6} m",
            live.overlap
        );
    }
}
