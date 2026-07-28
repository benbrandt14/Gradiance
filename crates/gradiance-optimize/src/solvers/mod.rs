//! The solver implementations.
//!
//! Adding a strategy is: a module here implementing
//! [`Solver`](crate::solver::Solver), a variant on
//! [`SolverKind`](crate::problem::SolverKind), and one row in [`build`].
//! Stopping rules and best-so-far tracking are *not* a solver's
//! business — [`PackRun`](crate::solver::PackRun) owns those for everyone.

pub mod descent;
pub mod naive;
pub mod shelf;

use crate::problem::{PackProblem, SolverKind};
use crate::solver::Solver;

/// Instantiates the configured solver, seeded for reproducibility and warm
/// started if the config asks for it.
pub fn build(problem: &PackProblem) -> Box<dyn Solver> {
    let mut solver: Box<dyn Solver> = match problem.config.solver {
        SolverKind::Shelf => Box::new(shelf::ShelfSolver::new(problem)),
        SolverKind::Descent => Box::new(descent::DescentSolver::new(problem)),
        SolverKind::Naive => Box::new(naive::NaiveSolver::new(problem)),
    };
    // The baseline is exempt on purpose. Handing it a constructive packing
    // to start from would be borrowing the answer from the very solver it is
    // supposed to be compared against, and the comparison would mean
    // nothing — a physics settle does not get a warm start either.
    if problem.config.warm_start && problem.config.solver != SolverKind::Naive {
        solver.seed(warm_start_layout(problem));
    }
    solver
}

/// A constructive shelf packing, used as an iterative solver's starting
/// point when [`PackConfig::warm_start`](crate::PackConfig::warm_start) is
/// set.
///
/// It matters most for [`SolverKind::Descent`], which strictly descends and
/// therefore inherits whatever basin it is dropped into — from a scattered
/// pile it converges to a scattered pile.
pub fn warm_start_layout(problem: &PackProblem) -> crate::problem::Layout {
    let mut shelf = shelf::ShelfSolver::new(problem);
    let mut scratch = crate::objective::Scratch::new(problem.len());
    shelf.step(problem, &mut scratch);
    shelf.layout().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::{Scratch, metrics};
    use crate::problem::{Layout, PackConfig, PackItem, RotationMode};
    use crate::solver::PackRun;
    use bevy::math::Vec2;

    /// A rectangle centred at `c` with half-extents `h`.
    fn rect(c: Vec2, h: Vec2) -> PackItem {
        PackItem::from_world_outline(
            &[
                c + Vec2::new(-h.x, -h.y),
                c + Vec2::new(h.x, -h.y),
                c + Vec2::new(h.x, h.y),
                c + Vec2::new(-h.x, h.y),
            ],
            0.0,
            1,
            false,
        )
    }

    /// A fixed-seed LCG, so the benchmark scenes are the same every run
    /// rather than whatever the last edit happened to produce. Six lines
    /// inline beats a module: this is its only caller.
    struct Rng(u64);

    impl Rng {
        fn unit(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            ((self.0 >> 33) as f32) / (u32::MAX >> 1) as f32
        }
        fn range(&mut self, lo: f32, hi: f32) -> f32 {
            lo + self.unit() * (hi - lo)
        }
    }

    /// Deterministic pseudo-random scatter for the benchmark scenes.
    fn scene(kind: &str) -> Vec<PackItem> {
        let mut rng = Rng(0xBEEF);
        match kind {
            // Uniform squares, far apart: the easy case every solver should
            // handle, and the one where a perfect tiling exists.
            "uniform" => (0..12)
                .map(|i| {
                    rect(
                        Vec2::new((i % 4) as f32 * 6.0, (i / 4) as f32 * 6.0),
                        Vec2::splat(0.5),
                    )
                })
                .collect(),
            // Mixed sizes: rewards actually reasoning about placement rather
            // than just pulling everything inward.
            "mixed" => (0..14)
                .map(|_| {
                    let s = 0.25 + rng.unit() * 0.7;
                    rect(
                        Vec2::new(rng.range(-14.0, 14.0), rng.range(-14.0, 14.0)),
                        Vec2::new(s, s * rng.range(0.5, 1.4)),
                    )
                })
                .collect(),
            // Long bars among squares: the case where orientation matters.
            _ => (0..10)
                .map(|i| {
                    if i % 2 == 0 {
                        rect(Vec2::new(i as f32 * 5.0, 0.0), Vec2::new(1.6, 0.2))
                    } else {
                        rect(Vec2::new(i as f32 * 5.0, 6.0), Vec2::splat(0.55))
                    }
                })
                .collect(),
        }
    }

    fn base_config(solver: SolverKind) -> PackConfig {
        PackConfig {
            solver,
            clearance: 0.0,
            rotation: RotationMode::Fixed,
            max_iterations: 2500,
            patience: 400,
            // Explicit rather than inherited: every comparison here has to
            // control for the warm start, or it is measuring the shelf.
            warm_start: solver.wants_warm_start(),
            ..Default::default()
        }
    }

    /// Solves a scene and returns `(fill, feasible)` for the applied layout.
    fn run(kind: &str, config: PackConfig) -> (f32, bool) {
        let problem = PackProblem::new(scene(kind), config);
        let mut run = PackRun::new(problem);
        run.solve();
        let mut scratch = Scratch::new(run.problem().len());
        let m = metrics(run.problem(), run.best_layout(), &mut scratch);
        (m.fill, m.is_feasible())
    }

    /// Every *real* solver must return a legal layout on every scene. A
    /// solver may return a poor arrangement; it may never return an illegal
    /// one.
    ///
    /// [`SolverKind::Naive`] is excluded on purpose — settling at a force
    /// balance leaves residual interpenetration, which is precisely the
    /// failure this crate exists to avoid and is asserted separately.
    #[test]
    fn every_real_solver_returns_a_collision_free_layout() {
        for kind in ["uniform", "mixed", "bars"] {
            for solver in [SolverKind::Shelf, SolverKind::Descent] {
                let (_, feasible) = run(kind, base_config(solver));
                assert!(feasible, "{solver:?} left overlaps on the {kind} scene");
            }
        }
    }

    /// The bar the user set: the real solvers must beat the naive
    /// force-attraction packing, not merely differ from it.
    ///
    /// The comparison is deliberately generous to the baseline — it runs
    /// through the same [`PackRun`] driver, so it gets best-so-far tracking
    /// that a genuine physics settle would not have, and the same iteration
    /// budget. It still loses, because it is optimizing nothing.
    #[test]
    fn the_real_solvers_beat_the_naive_baseline_on_density() {
        for kind in ["uniform", "mixed", "bars"] {
            let (naive_fill, _) = run(kind, base_config(SolverKind::Naive));
            for solver in [SolverKind::Shelf, SolverKind::Descent] {
                let (fill, feasible) = run(kind, base_config(solver));
                assert!(feasible);
                assert!(
                    fill > naive_fill * 1.15,
                    "{solver:?} only reached {fill:.3} fill on the {kind} scene \
                     against the naive baseline's {naive_fill:.3} — the whole \
                     crate has to justify itself against that number"
                );
            }
        }
    }

    #[test]
    fn packing_reaches_a_respectable_fill_ratio() {
        // Twelve equal squares admit a perfect tiling; anything under two
        // thirds means the solver is leaving obvious space on the table.
        for solver in [SolverKind::Shelf, SolverKind::Descent] {
            let (fill, _) = run("uniform", base_config(solver));
            assert!(
                fill > 0.66,
                "{solver:?} reached only {fill:.3} fill on a scene that tiles perfectly"
            );
        }
    }

    /// A strong gap weight must not blow the arrangement apart.
    ///
    /// Regression test for a real design bug. The gap term was originally
    /// scored over "every pair within radius R", which has a trivial
    /// exploit: push everything far enough apart that no pair is within R
    /// and the term reads zero — so the solver discovered that the tightest
    /// packing is an explosion, and a strong weight collapsed fill from 0.90
    /// to 0.04. Scoring a fixed *count* of nearest neighbours instead cannot
    /// empty out, so spreading always costs.
    #[test]
    fn a_strong_gap_weight_cannot_be_escaped_by_spreading_out() {
        for kind in ["uniform", "mixed", "bars"] {
            let baseline = run(kind, base_config(SolverKind::Descent)).0;
            let mut greedy = base_config(SolverKind::Descent);
            greedy.weights.gap = 3.0;
            let (fill, feasible) = run(kind, greedy);
            assert!(feasible);
            assert!(
                fill > baseline * 0.8,
                "a strong gap weight collapsed the {kind} packing: \
                 {baseline:.3} -> {fill:.3}"
            );
        }
    }

    #[test]
    fn a_warm_start_rescues_gradient_descent() {
        // Descent strictly descends, so from a scattered pile it converges to
        // that pile's local minimum. This is the reason `wants_warm_start`
        // exists, stated as a test.
        for kind in ["uniform", "mixed", "bars"] {
            let cold = run(
                kind,
                PackConfig {
                    warm_start: false,
                    ..base_config(SolverKind::Descent)
                },
            )
            .0;
            let warm = run(kind, base_config(SolverKind::Descent)).0;
            assert!(
                warm >= cold - 1e-3,
                "{kind}: a warm start must never cost descent density: \
                 cold {cold:.3} -> warm {warm:.3}"
            );
        }
    }

    #[test]
    fn the_warm_start_layout_is_itself_a_legal_packing() {
        let problem = PackProblem::new(
            scene("mixed"),
            PackConfig {
                warm_start: true,
                ..base_config(SolverKind::Descent)
            },
        );
        let layout = warm_start_layout(&problem);
        let mut scratch = Scratch::new(problem.len());
        assert!(metrics(&problem, &layout, &mut scratch).is_feasible());
    }

    #[test]
    fn seeding_replaces_the_working_layout_for_iterative_solvers() {
        let problem = PackProblem::new(scene("uniform"), base_config(SolverKind::Descent));
        let mut solver = build(&problem);
        let seeded = warm_start_layout(&problem);
        solver.seed(seeded.clone());
        assert_eq!(solver.layout().poses, seeded.poses);
    }

    #[test]
    fn the_parallel_weight_lines_bars_up() {
        // Bars at assorted angles, with rotation allowed: turning on the
        // alignment term must raise the measured alignment.
        // The outline must be genuinely turned in world space — passing the
        // same axis-aligned rectangle with different `rot` values describes
        // the same placed bar every time.
        let bar = |angle: f32, at: Vec2| {
            let (sin, cos) = angle.sin_cos();
            let pts: Vec<Vec2> = [(-1.2, -0.15), (1.2, -0.15), (1.2, 0.15), (-1.2, 0.15)]
                .into_iter()
                .map(|(x, y)| at + Vec2::new(x * cos - y * sin, x * sin + y * cos))
                .collect();
            PackItem::from_world_outline(&pts, angle, 1, false)
        };
        let bars: Vec<PackItem> = (0..8)
            .map(|i| {
                bar(
                    i as f32 * 0.4,
                    Vec2::new((i % 4) as f32 * 4.0, (i / 4) as f32 * 4.0),
                )
            })
            .collect();
        let measure_alignment = |parallel: f32| {
            let mut config = base_config(SolverKind::Descent);
            config.rotation = RotationMode::Free;
            config.weights.parallel = parallel;
            let problem = PackProblem::new(bars.clone(), config);
            let mut run = PackRun::new(problem);
            run.solve();
            let mut scratch = Scratch::new(run.problem().len());
            metrics(run.problem(), run.best_layout(), &mut scratch).alignment
        };
        let off = measure_alignment(0.0);
        let on = measure_alignment(6.0);
        assert!(
            on > off,
            "the alignment weight should tidy the arrangement: {off:.3} -> {on:.3}"
        );
    }

    #[test]
    fn a_positive_contact_weight_reduces_touching() {
        // Inside a fixed box (so the set cannot simply fly apart), asking for
        // less contact must actually produce less contact.
        let items: Vec<PackItem> = (0..9)
            .map(|i| rect(Vec2::new((i % 3) as f32, (i / 3) as f32), Vec2::splat(0.45)))
            .collect();
        let measure_contact = |contact: f32| {
            let mut config = base_config(SolverKind::Descent);
            config.boundary = crate::problem::Boundary::Rect {
                width: 6.0,
                height: 6.0,
            };
            config.weights.contact = contact;
            config.weights.extent = 0.0;
            config.weights.fill = 0.0;
            config.weights.gap = 0.0;
            let problem = PackProblem::new(items.clone(), config);
            let mut run = PackRun::new(problem);
            run.solve();
            let mut scratch = Scratch::new(run.problem().len());
            metrics(run.problem(), run.best_layout(), &mut scratch).contact
        };
        assert!(
            measure_contact(4.0) <= measure_contact(0.0),
            "asking for less contact must not produce more of it"
        );
    }

    #[test]
    fn a_layout_is_never_returned_worse_than_the_start() {
        // Whatever the solver, the run's best-so-far protects the user.
        for solver in SolverKind::ALL {
            let problem = PackProblem::new(scene("mixed"), base_config(solver));
            let start = Layout::from_starts(&problem.items);
            let mut scratch = Scratch::new(problem.len());
            let before = metrics(&problem, &start, &mut scratch).objective;
            let mut run = PackRun::new(problem);
            run.solve();
            assert!(
                run.report().best.objective <= before + 1e-3,
                "{solver:?} returned a worse layout than it was given"
            );
        }
    }
}
