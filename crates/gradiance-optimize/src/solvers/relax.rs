//! Pulse-compaction relaxation — the default packer.
//!
//! Every iteration runs a Jacobi **separation** sweep: each violating pair
//! contributes half of its minimum translation to each partner (all of it
//! when the partner is pinned). Corrections are accumulated and applied at
//! the end of the sweep rather than in place, so a dense pile does not
//! depend on item order.
//!
//! Whenever that sweep finds the arrangement **settled** (total overlap
//! under `settle_epsilon`), it is joined by a **compaction pulse**: every
//! item is pulled toward the arrangement centre by a fraction of its
//! distance, scaled by the `extent + fill` goal weights. Settling bias, a
//! small random kick (to break symmetric standoffs), and orientation changes
//! ride the same pulse.
//!
//! # Why pulse instead of pulling every iteration
//!
//! Applying both forces on every iteration is the obvious design and it does
//! not work: separation and attraction settle into a *force balance* rather
//! than a legal layout, leaving a permanent residual overlap proportional to
//! the attraction gain. The run then reports convergence on an arrangement
//! whose bodies are inside each other. Alternating the two phases fixes it —
//! each squeeze is followed by however many pure-separation iterations it
//! takes to drive the overlap back to zero, so the layout is legal again
//! before the next squeeze and the best-so-far is always taken from a
//! settled moment. The preview reads as a clenching fist rather than a
//! jitter.
//!
//! The gate is on measured overlap rather than a fixed iteration count for a
//! concrete reason: how long settling takes depends entirely on how tangled
//! the input was. A fixed period re-squeezes a deeply interpenetrating pile
//! before it has come apart, and the run never reaches a legal state at all.
//!
//! Velocity carries between iterations (`inertia`), so a pile keeps flowing
//! through a tight gap instead of stopping at first contact. None of this is
//! a physics step: there is no mass, no restitution, no time, and no avian
//! solver involved — displacements are geometric corrections, and the
//! per-iteration total is hard-clamped to `max_step` so the preview never
//! teleports.

use bevy::math::Vec2;
use gradiance_core::units::PosRot;

use crate::objective::{Scratch, clamp_to_boundary};
use crate::problem::{Layout, PackProblem};
use crate::rng::Rng;
use crate::solver::Solver;
use gradiance_geometry::sat::separation;

/// Extra separation applied to every violating pair each iteration, in
/// metres.
///
/// The gain term alone only ever removes a *fraction* of a penetration, so a
/// contact approaches zero geometrically and never actually clears — the
/// arrangement would sit forever a few microns short of legal and never earn
/// a compaction pulse. A fixed absolute nudge on top guarantees each pair
/// gains real ground every iteration and terminates.
const SEPARATION_SLOP: f32 = 1e-5;

/// The default `extent + fill`, so default weights reproduce a plain
/// `attraction` pull and the dial is a multiplier rather than a rescale.
const WEIGHT_REF: f32 = 2.0;

/// Iterative separation/attraction relaxation.
pub struct RelaxSolver {
    layout: Layout,
    velocity: Vec<Vec2>,
    correction: Vec<Vec2>,
    target: Vec2,
    rng: Rng,
}

impl RelaxSolver {
    /// Starts from the problem's current arrangement.
    pub fn new(problem: &PackProblem, seed: u64) -> Self {
        Self {
            layout: Layout::from_starts(&problem.items),
            velocity: vec![Vec2::ZERO; problem.items.len()],
            correction: vec![Vec2::ZERO; problem.items.len()],
            target: problem.start_center(),
            rng: Rng::new(seed),
        }
    }
}

impl RelaxSolver {
    /// One pass over every pair that is either overlapping (separate it) or
    /// merely near (remember the gap). Returns the total penetration, which
    /// is what gates the next compaction pulse.
    fn sweep_pairs(&mut self, problem: &PackProblem, scratch: &Scratch) -> f32 {
        let n = problem.items.len();
        let cfg = &problem.config;
        let params = cfg.relax;
        let mut total_overlap = 0.0;
        // One pass over every pair close enough to matter.
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
                let Some(sep) = separation(scratch.placed(i), scratch.placed(j)) else {
                    continue;
                };

                if sep.distance < cfg.clearance {
                    let depth = cfg.clearance - sep.distance;
                    total_overlap += depth;
                    let push = sep.axis * depth.mul_add(params.separation_gain, SEPARATION_SLOP);
                    // A pinned partner absorbs none of the correction, so the
                    // movable one takes the whole exit.
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
        }

        total_overlap
    }
}

impl Solver for RelaxSolver {
    fn name(&self) -> &'static str {
        "relaxation"
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
        self.velocity.resize(n, Vec2::ZERO);

        // The pulse strength, scaled by the goal weights so the dial means
        // something here and not only in the score.
        let global_gain =
            params.attraction * (cfg.weights.extent + cfg.weights.fill).max(0.0) / WEIGHT_REF;
        // A local, neighbour-directed pull was tried here as well and
        // measured *worse* on every benchmark scene: it forms tight clumps
        // that each close their own gaps while the arrangement as a whole
        // stays loose. The gap weight remains an objective goal — it is how
        // you ask for a particular neighbour spacing — but it does not steer
        // this solver's pulse. See `docs/optimize-decision.md`.

        let total_overlap = self.sweep_pairs(problem, scratch);

        // 2. The compaction pulse — only once the arrangement has settled, so
        //    every squeeze starts from a legal layout and the iterations in
        //    between are free to drive the overlap to zero (module docs).
        let pulse = total_overlap <= params.settle_epsilon;
        // The direction every item is asked to line up with, if anyone asked.
        let dominant = (pulse && cfg.weights.parallel > 0.0)
            .then(|| dominant_direction(scratch, cfg.alignment))
            .flatten();
        if pulse {
            for i in 0..n {
                if !problem.movable(i) {
                    continue;
                }
                let pos = self.layout.poses[i].pos;
                self.correction[i] += (self.target - pos) * global_gain;
                self.correction[i] += cfg.gravity_bias * params.attraction;
                if params.jitter > 0.0 {
                    self.correction[i] +=
                        Vec2::new(self.rng.signed(), self.rng.signed()) * params.jitter;
                }
            }
        }
        // Integrate: momentum, clamp, boundary projection.
        for i in 0..n {
            if !problem.movable(i) {
                continue;
            }
            let v = self.velocity[i] * params.inertia + self.correction[i];
            let v = if v.length() > cfg.max_step {
                v.normalize_or_zero() * cfg.max_step
            } else {
                v
            };
            self.velocity[i] = v;
            let Some(item) = problem.items.get(i) else {
                continue;
            };
            let mut pose = PosRot {
                pos: self.layout.poses[i].pos + v,
                rot: self.layout.poses[i].rot,
            };
            // Turning is a compaction-phase decision too: re-orienting during
            // a settle would keep re-creating the overlaps it is clearing.
            if pulse && cfg.rotation.allows_rotation() && params.rotation_gain > 0.0 {
                pose.rot = match dominant {
                    // Someone asked for parallel edges: turn toward the
                    // arrangement's own dominant direction rather than at the
                    // tidiest footprint, which is a different goal.
                    Some(direction) => {
                        aligned_rotation(problem, i, pose, direction, params.rotation_gain, scratch)
                    }
                    None => relaxed_rotation(problem, i, pose, params.rotation_gain, &mut self.rng),
                };
            }
            pose.pos = clamp_to_boundary(problem, pose, item.radius);
            self.layout.poses[i] = pose;
        }
    }
}

/// The arrangement's dominant edge direction, as a folded angle.
///
/// The circular mean of every hull edge's direction, weighted by edge length
/// and folded by [`EdgeAlignment`] — the same quantity
/// [`crate::objective`] scores, so what the solver steers toward and what the
/// score rewards cannot drift apart. `None` when there are no edges to speak
/// of.
fn dominant_direction(scratch: &Scratch, mode: crate::problem::EdgeAlignment) -> Option<f32> {
    let k = mode.fold() as f32;
    let mut acc = Vec2::ZERO;
    let mut total = 0.0;
    for i in 0..scratch.len() {
        let poly = scratch.placed(i);
        if poly.len() < 2 {
            continue;
        }
        for v in 0..poly.len() {
            let edge = poly[(v + 1) % poly.len()] - poly[v];
            let len = edge.length();
            if len < 1e-9 {
                continue;
            }
            let (sin, cos) = (edge.y.atan2(edge.x) * k).sin_cos();
            acc += Vec2::new(cos, sin) * len;
            total += len;
        }
    }
    (total > 1e-9 && acc.length() > 1e-9).then(|| acc.y.atan2(acc.x) / k)
}

/// Turns an item toward `direction`, blended by `gain`.
///
/// Both the item's own folded edge direction and the target are compared
/// modulo the fold, so an item never spins the long way round to reach an
/// orientation it is already equivalent to.
fn aligned_rotation(
    problem: &PackProblem,
    index: usize,
    pose: PosRot,
    direction: f32,
    gain: f32,
    scratch: &Scratch,
) -> f32 {
    let Some(item) = problem.items.get(index) else {
        return pose.rot;
    };
    let mode = problem.config.rotation;
    let fold = problem.config.alignment.fold() as f32;
    let period = std::f32::consts::TAU / fold;
    let Some(own) = dominant_of(scratch.placed(index), fold) else {
        return pose.rot;
    };
    // Shortest turn that maps this item's edge family onto the target's.
    let mut delta = direction - own;
    delta -= (delta / period).round() * period;
    mode.snap(pose.rot + delta * gain, item.start.rot)
}

/// One polygon's folded edge direction.
fn dominant_of(poly: &[Vec2], fold: f32) -> Option<f32> {
    if poly.len() < 2 {
        return None;
    }
    let mut acc = Vec2::ZERO;
    for v in 0..poly.len() {
        let edge = poly[(v + 1) % poly.len()] - poly[v];
        let len = edge.length();
        if len < 1e-9 {
            continue;
        }
        let (sin, cos) = (edge.y.atan2(edge.x) * fold).sin_cos();
        acc += Vec2::new(cos, sin) * len;
    }
    (acc.length() > 1e-9).then(|| acc.y.atan2(acc.x) / fold)
}

/// Nudges an item's orientation toward the allowed angle whose footprint
/// costs least, blended by `gain`.
///
/// With a quantized rotation mode the search is over the (few) allowed
/// orientations, so this is a cheap discrete improvement rather than a
/// gradient. With free rotation there is no preferred angle to snap to, so
/// it applies a small random turn instead and lets the objective decide
/// whether it survives.
fn relaxed_rotation(
    problem: &PackProblem,
    index: usize,
    pose: PosRot,
    gain: f32,
    rng: &mut Rng,
) -> f32 {
    let Some(item) = problem.items.get(index) else {
        return pose.rot;
    };
    let mode = problem.config.rotation;
    if !mode.allows_rotation() {
        return item.start.rot;
    }
    if matches!(mode, crate::problem::RotationMode::Free) {
        return pose.rot + rng.signed() * gain * 0.1;
    }
    // Prefer the allowed orientation with the smallest axis-aligned
    // footprint: rows and columns pack far better out of squat parts.
    // Quarter is the fallback too: `Fixed` and `Free` returned above, so the
    // wildcard is unreachable in practice.
    let steps: u32 = match mode {
        crate::problem::RotationMode::Steps { steps } => steps.max(1),
        _ => 4,
    };
    let step = std::f32::consts::TAU / steps as f32;
    let mut best = pose.rot;
    let mut best_cost = f32::MAX;
    let mut buf = Vec::new();
    for k in 0..steps {
        let candidate = item.start.rot + step * k as f32;
        let size = item.footprint(candidate, &mut buf);
        let cost = size.x * size.y + size.max_element();
        if cost < best_cost {
            best_cost = cost;
            best = candidate;
        }
    }
    // Blend toward the winner so the preview turns visibly rather than
    // snapping, then re-snap so the layout only ever holds legal angles.
    let blended = pose.rot + (best - pose.rot) * gain;
    mode.snap(blended, item.start.rot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::{Scratch, metrics};
    use crate::problem::{LayerRule, PackConfig, PackItem, RotationMode, SolverKind};
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
            solver: SolverKind::Relax,
            clearance: 0.0,
            rotation: RotationMode::Fixed,
            max_iterations: 4000,
            patience: 400,
            ..Default::default()
        }
    }

    #[test]
    fn overlapping_squares_are_pushed_apart_until_legal() {
        // Four unit squares stacked nearly on top of each other.
        let items = (0..4)
            .map(|i| square(Vec2::new(i as f32 * 0.1, i as f32 * 0.05), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        let report = run.report();
        assert!(report.start.violations > 0, "the start really did overlap");
        assert!(
            report.best.is_feasible(),
            "relaxation must reach a collision-free layout (overlap {}, {} pairs)",
            report.best.overlap,
            report.best.violations
        );
    }

    #[test]
    fn a_scattered_row_is_drawn_together() {
        // Five squares spread far apart: relaxation should shrink the
        // bounding box substantially while staying legal.
        let items = (0..5)
            .map(|i| square(Vec2::new(i as f32 * 4.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        let report = run.report();
        assert!(report.best.is_feasible(), "still collision-free");
        assert!(
            report.shrinkage() > 0.5,
            "expected a big reduction, got {:.3} (start {:.3} → best {:.3})",
            report.shrinkage(),
            report.start.extent,
            report.best.extent
        );
    }

    #[test]
    fn clearance_is_honored_in_the_result() {
        let items = (0..4)
            .map(|i| square(Vec2::new(i as f32 * 3.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                clearance: 0.25,
                ..config()
            },
        ));
        run.solve();
        assert!(
            run.report().best.is_feasible(),
            "the 0.25 m gap must survive into the answer"
        );
        // Independently re-check the gap with a fresh evaluation.
        let mut scratch = Scratch::new(run.problem().len());
        let m = metrics(run.problem(), run.best_layout(), &mut scratch);
        assert!(
            m.is_feasible(),
            "{:.6} m of overlap across {} pairs",
            m.overlap,
            m.violations
        );
    }

    #[test]
    fn pinned_items_never_move_and_others_pack_around_them() {
        let mut anchor = square(Vec2::ZERO, 1.0);
        anchor.pinned = true;
        let anchor_start = anchor.start;
        let mut items = vec![anchor];
        items.extend((0..4).map(|i| square(Vec2::new(3.0 + i as f32 * 2.0, 0.0), 0.5)));
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        let poses = &run.best_layout().poses;
        assert!(
            poses[0].pos.distance(anchor_start.pos) < 1e-6,
            "a pinned item is an obstacle, not a participant"
        );
        assert!(run.report().best.is_feasible());
    }

    #[test]
    fn depth_layers_let_bodies_stack_in_the_same_footprint() {
        // Two squares on disjoint layers start coincident. A depth-aware run
        // must leave them there; a flat run must separate them.
        let stacked = || {
            vec![
                PackItem::from_world_outline(
                    &[
                        Vec2::new(-0.5, -0.5),
                        Vec2::new(0.5, -0.5),
                        Vec2::new(0.5, 0.5),
                        Vec2::new(-0.5, 0.5),
                    ],
                    0.0,
                    0b0001,
                    false,
                ),
                PackItem::from_world_outline(
                    &[
                        Vec2::new(-0.4, -0.5),
                        Vec2::new(0.6, -0.5),
                        Vec2::new(0.6, 0.5),
                        Vec2::new(-0.4, 0.5),
                    ],
                    0.0,
                    0b0010,
                    false,
                ),
            ]
        };
        let mut aware = PackRun::new(PackProblem::new(
            stacked(),
            PackConfig {
                layers: LayerRule::Respect,
                ..config()
            },
        ));
        aware.solve();
        let overlap_kept = {
            let p = aware.best_layout().poses.clone();
            p[0].pos.distance(p[1].pos)
        };
        assert!(
            overlap_kept < 0.5,
            "off-layer bodies should stay stacked, ended {overlap_kept:.3} apart"
        );

        let mut flat = PackRun::new(PackProblem::new(
            stacked(),
            PackConfig {
                layers: LayerRule::Solid,
                ..config()
            },
        ));
        flat.solve();
        let p = &flat.best_layout().poses;
        assert!(
            p[0].pos.distance(p[1].pos) > 0.9,
            "flat mode must separate them by about a full body"
        );
        assert!(flat.report().best.is_feasible());
    }

    #[test]
    fn a_hard_rectangle_contains_the_result() {
        let items = (0..4)
            .map(|i| square(Vec2::new(i as f32 * 3.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                boundary: crate::problem::Boundary::Rect {
                    width: 6.0,
                    height: 6.0,
                },
                ..config()
            },
        ));
        run.solve();
        let center = run.problem().start_center();
        for pose in &run.best_layout().poses {
            let d = (pose.pos - center).abs();
            assert!(
                d.x <= 3.0 + 1e-3 && d.y <= 3.0 + 1e-3,
                "escaped the box at {pose:?}"
            );
        }
    }

    #[test]
    fn no_item_moves_further_than_max_step_in_one_iteration() {
        let items = (0..4)
            .map(|i| square(Vec2::new(i as f32 * 8.0, 0.0), 0.5))
            .collect();
        let problem = PackProblem::new(
            items,
            PackConfig {
                max_step: 0.05,
                // The step limit is about what one *iteration* does; a warm
                // start replaces the layout before any iteration runs.
                warm_start: false,
                ..config()
            },
        );
        let before = Layout::from_starts(&problem.items);
        let mut run = PackRun::new(problem);
        run.step();
        for (a, b) in before.poses.iter().zip(&run.working_layout().poses) {
            assert!(
                a.pos.distance(b.pos) <= 0.05 + 1e-5,
                "one iteration jumped {:.4}",
                a.pos.distance(b.pos)
            );
        }
    }

    #[test]
    fn fixed_rotation_leaves_every_angle_untouched() {
        let items = (0..4)
            .map(|i| {
                PackItem::from_world_outline(
                    &[
                        Vec2::new(-1.0, -0.25),
                        Vec2::new(1.0, -0.25),
                        Vec2::new(1.0, 0.25),
                        Vec2::new(-1.0, 0.25),
                    ],
                    0.3 * i as f32,
                    1,
                    false,
                )
            })
            .collect::<Vec<_>>();
        let starts: Vec<f32> = items.iter().map(|i| i.start.rot).collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        for (pose, start) in run.best_layout().poses.iter().zip(starts) {
            assert!((pose.rot - start).abs() < 1e-6);
        }
    }

    #[test]
    fn quarter_rotation_only_ever_emits_quarter_turns() {
        let items = (0..5)
            .map(|i| {
                PackItem::from_world_outline(
                    &[
                        Vec2::new(-1.5, -0.2),
                        Vec2::new(1.5, -0.2),
                        Vec2::new(1.5, 0.2),
                        Vec2::new(-1.5, 0.2),
                    ],
                    0.0,
                    1,
                    false,
                )
                .tap_translate(Vec2::new(i as f32 * 0.3, i as f32 * 0.1))
            })
            .collect::<Vec<_>>();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                rotation: RotationMode::Quarter,
                ..config()
            },
        ));
        run.solve();
        let quarter = std::f32::consts::FRAC_PI_2;
        for pose in &run.best_layout().poses {
            let k = pose.rot / quarter;
            assert!(
                (k - k.round()).abs() < 1e-4,
                "angle {} is not a quarter turn",
                pose.rot
            );
        }
    }

    /// Test helper: shifts an item's start position.
    impl PackItem {
        fn tap_translate(mut self, by: Vec2) -> Self {
            self.start.pos += by;
            self
        }
    }
}
