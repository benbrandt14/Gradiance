//! Metropolis simulated annealing.
//!
//! The only solver here that can climb out of a bad local minimum. Each
//! iteration proposes one small change — nudge an item, turn an item, or
//! swap two items' positions — scores it, and accepts it outright if it
//! improved or with probability `exp(-ΔE/T)` if it did not. `T` starts high
//! (bad moves are routinely accepted, so the arrangement genuinely explores)
//! and decays geometrically until only improvements survive.
//!
//! Two design notes worth keeping:
//!
//! - **Temperature is relative.** `start_temperature` multiplies the
//!   *initial energy*, so the same setting behaves the same whether the
//!   selection is a 10 cm cluster or a 40 m field. An absolute temperature
//!   would be unusable across scenes.
//! - **Swaps matter more than nudges.** A pile whose big and small parts
//!   started in the wrong places cannot fix itself by local motion — every
//!   intermediate state is worse. Position swaps jump straight over that
//!   barrier, which is most of why annealing beats relaxation when it does.
//!
//! The run's best-so-far is tracked by [`PackRun`](crate::solver::PackRun),
//! not here, so an exploration excursion can never lose a good arrangement.

use bevy::math::Vec2;

use crate::objective::{Scratch, clamp_to_boundary, metrics};
use crate::problem::{Layout, PackProblem};
use crate::rng::Rng;
use crate::solver::Solver;

/// Metropolis annealing over positions, rotations, and swaps.
pub struct AnnealSolver {
    layout: Layout,
    movable: Vec<usize>,
    energy: f32,
    /// Energy of the starting arrangement — the yardstick that makes
    /// `start_temperature` scale-free.
    energy_scale: f32,
    temperature: f32,
    /// Where compaction proposals pull toward.
    center: Vec2,
    rng: Rng,
    initialized: bool,
}

impl AnnealSolver {
    /// Starts from the problem's current arrangement.
    pub fn new(problem: &PackProblem, seed: u64) -> Self {
        Self {
            layout: Layout::from_starts(&problem.items),
            movable: (0..problem.len()).filter(|i| problem.movable(*i)).collect(),
            energy: f32::MAX,
            energy_scale: 1.0,
            temperature: problem.config.anneal.start_temperature,
            center: problem.start_center(),
            rng: Rng::new(seed),
            initialized: false,
        }
    }
}

/// What one iteration proposes.
enum Proposal {
    /// Move (and possibly turn) one item; carries the item's previous pose.
    Nudge(usize, gradiance_core::units::PosRot),
    /// Exchange two items' positions; carries both previous poses.
    Swap(
        usize,
        gradiance_core::units::PosRot,
        usize,
        gradiance_core::units::PosRot,
    ),
    /// Nothing to do (no movable items).
    None,
}

impl Solver for AnnealSolver {
    fn name(&self) -> &'static str {
        "annealing"
    }

    fn layout(&self) -> &Layout {
        &self.layout
    }

    fn seed(&mut self, layout: Layout) {
        self.layout = layout;
    }

    fn step(&mut self, problem: &PackProblem, scratch: &mut Scratch) {
        let params = problem.config.anneal;
        if !self.initialized {
            self.initialized = true;
            self.energy = metrics(problem, &self.layout, scratch).objective;
            // A zero-extent start (every item degenerate) would make the
            // temperature meaningless; fall back to a unit scale.
            self.energy_scale = if self.energy.is_finite() && self.energy.abs() > 1e-9 {
                self.energy.abs()
            } else {
                1.0
            };
        }

        let proposal = self.propose(problem);
        if matches!(proposal, Proposal::None) {
            return;
        }

        let candidate = metrics(problem, &self.layout, scratch).objective;
        let delta = candidate - self.energy;
        let temperature = (self.temperature * self.energy_scale).max(1e-12);
        let accept = delta <= 0.0 || self.rng.unit() < (-delta / temperature).exp();

        if accept {
            self.energy = candidate;
        } else {
            match proposal {
                Proposal::Nudge(i, old) => self.layout.poses[i] = old,
                Proposal::Swap(i, a, j, b) => {
                    self.layout.poses[i] = a;
                    self.layout.poses[j] = b;
                }
                Proposal::None => {}
            }
        }

        self.temperature *= params.cooling;
    }
}

impl AnnealSolver {
    /// Mutates the layout with one random proposal, returning what to undo
    /// if it is rejected.
    fn propose(&mut self, problem: &PackProblem) -> Proposal {
        if self.movable.is_empty() {
            return Proposal::None;
        }
        let params = problem.config.anneal;
        let cfg = &problem.config;

        if self.movable.len() >= 2 && self.rng.unit() < params.swap_probability {
            let a = self.movable[self.rng.index(self.movable.len())];
            let mut b = self.movable[self.rng.index(self.movable.len())];
            if a == b {
                b = self.movable[(self.movable.iter().position(|i| *i == a).unwrap_or(0) + 1)
                    % self.movable.len()];
            }
            if a == b {
                return Proposal::None;
            }
            let (old_a, old_b) = (self.layout.poses[a], self.layout.poses[b]);
            // Exchange positions only — each item keeps its own orientation,
            // which is what makes a swap a re-ordering rather than a reshape.
            self.layout.poses[a].pos = old_b.pos;
            self.layout.poses[b].pos = old_a.pos;
            for index in [a, b] {
                if let Some(item) = problem.items.get(index) {
                    self.layout.poses[index].pos =
                        clamp_to_boundary(problem, self.layout.poses[index], item.radius);
                }
            }
            return Proposal::Swap(a, old_a, b, old_b);
        }

        let i = self.movable[self.rng.index(self.movable.len())];
        let old = self.layout.poses[i];
        let step = Vec2::new(self.rng.gaussian(), self.rng.gaussian()) * params.move_scale;
        let step = if step.length() > cfg.max_step {
            step.normalize_or_zero() * cfg.max_step
        } else {
            step
        };
        let mut pose = old;
        pose.pos += step;
        // Lean the *proposal distribution* inward and along any settling
        // bias, rather than adding fields to the energy: it costs nothing per
        // proposal and it is what makes shrinking moves common enough for the
        // acceptance test to have something worth judging.
        pose.pos += (self.center - old.pos) * params.compaction;
        pose.pos += cfg.gravity_bias * params.move_scale;
        if cfg.rotation.allows_rotation() && params.rotation_scale > 0.0 {
            let turn = self.rng.gaussian() * params.rotation_scale;
            if let Some(item) = problem.items.get(i) {
                pose.rot = cfg.rotation.snap(pose.rot + turn, item.start.rot);
            }
        }
        if let Some(item) = problem.items.get(i) {
            pose.pos = clamp_to_boundary(problem, pose, item.radius);
        }
        self.layout.poses[i] = pose;
        Proposal::Nudge(i, old)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::{Scratch, metrics};
    use crate::problem::{PackConfig, PackItem, PackProblem, RotationMode, SolverKind};
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
            solver: SolverKind::Anneal,
            clearance: 0.0,
            rotation: RotationMode::Fixed,
            max_iterations: 4000,
            patience: 4000,
            ..Default::default()
        }
    }

    #[test]
    fn annealing_shrinks_a_scattered_set() {
        let items = (0..6)
            .map(|i| square(Vec2::new(i as f32 * 5.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        let report = run.report();
        assert!(report.best.is_feasible());
        assert!(
            report.shrinkage() > 0.3,
            "expected real progress, got {:.3}",
            report.shrinkage()
        );
    }

    #[test]
    fn the_result_is_never_worse_than_the_starting_arrangement() {
        // Annealing accepts uphill moves; the run's best-so-far must still
        // protect the user from ever getting a worse layout than they had.
        let items = (0..5)
            .map(|i| square(Vec2::new(i as f32 * 1.2, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                max_iterations: 300,
                ..config()
            },
        ));
        run.solve();
        let report = run.report();
        assert!(report.best.objective <= report.start.objective + 1e-4);
    }

    #[test]
    fn the_same_seed_reproduces_the_same_arrangement() {
        let items = || {
            (0..5)
                .map(|i| square(Vec2::new(i as f32 * 3.0, 0.0), 0.5))
                .collect::<Vec<_>>()
        };
        let solve = |seed: u64| {
            let mut run = PackRun::new(PackProblem::new(
                items(),
                PackConfig {
                    seed,
                    max_iterations: 200,
                    // Both seeds would otherwise start from the same
                    // constructive layout and converge to it.
                    warm_start: false,
                    ..config()
                },
            ));
            run.solve();
            run.best_layout().clone()
        };
        assert_eq!(solve(11), solve(11), "same seed, same answer");
        assert_ne!(
            solve(11),
            solve(12),
            "different seeds should explore differently"
        );
    }

    #[test]
    fn restarts_keep_the_best_attempt() {
        let items = (0..6)
            .map(|i| square(Vec2::new(i as f32 * 4.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                max_iterations: 150,
                patience: 150,
                restarts: 4,
                ..config()
            },
        ));
        run.solve();
        let report = run.report();
        assert_eq!(report.restart, 3, "all four attempts ran");
        assert!(
            report.total_iterations > 150,
            "restarts accumulate iterations"
        );
        assert!(report.best.objective <= report.start.objective + 1e-4);
    }

    #[test]
    fn a_hard_circle_contains_every_proposal() {
        let items = (0..5)
            .map(|i| square(Vec2::new(i as f32 * 2.0, 0.0), 0.4))
            .collect();
        let problem = PackProblem::new(
            items,
            PackConfig {
                boundary: crate::problem::Boundary::Circle { radius: 3.0 },
                max_iterations: 400,
                ..config()
            },
        );
        let center = problem.start_center();
        let mut run = PackRun::new(problem);
        run.solve();
        for pose in &run.best_layout().poses {
            assert!(
                pose.pos.distance(center) <= 3.0 + 1e-3,
                "escaped the circle at {pose:?}"
            );
        }
    }

    #[test]
    fn pinned_items_are_never_proposed() {
        let mut anchor = square(Vec2::ZERO, 1.0);
        anchor.pinned = true;
        let start = anchor.start;
        let mut items = vec![anchor];
        items.extend((0..4).map(|i| square(Vec2::new(4.0 + i as f32 * 2.0, 0.0), 0.5)));
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        assert!(run.best_layout().poses[0].pos.distance(start.pos) < 1e-6);
    }

    #[test]
    fn the_reported_best_layout_really_scores_what_the_report_says() {
        let items = (0..5)
            .map(|i| square(Vec2::new(i as f32 * 3.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        let mut scratch = Scratch::new(run.problem().len());
        let recomputed = metrics(run.problem(), run.best_layout(), &mut scratch);
        assert!((recomputed.objective - run.report().best.objective).abs() < 1e-3);
    }
}
