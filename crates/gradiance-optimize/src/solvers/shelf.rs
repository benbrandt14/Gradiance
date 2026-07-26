//! Constructive shelf packing — the instant, deterministic baseline.
//!
//! Classic rectangle packing: sort the items (largest first by default),
//! then lay them into horizontal rows, each item taking the first position
//! that fits. Unlike the iterative solvers this one **discards** the current
//! arrangement and builds a new one, which is exactly what you want when the
//! selection is a scattered pile with no structure worth preserving.
//!
//! Two deliberate simplifications:
//!
//! - Placement reasons about each item's **axis-aligned bounding box**, not
//!   its hull. Boxes are what shelves are for, and the direction of the
//!   error is safe: disjoint boxes imply disjoint hulls, so a shelf result
//!   is always collision-free, just not as tight as relaxation can get.
//! - Candidate positions are the row's left edge plus the right edge of
//!   every already-placed box, which is the standard exact candidate set for
//!   bottom-left packing — no scanning, no grid.
//!
//! Depth awareness falls out of the same scan: a box only blocks a
//! candidate position when its collision-layer bits actually intersect the
//! incoming item's, so a body on a different depth band simply does not
//! participate and the new item lands on top of it.

use bevy::math::Vec2;
use gradiance_core::units::PosRot;

use crate::hull::{bounds, place_into};
use crate::objective::Scratch;
use crate::problem::{Boundary, Layout, PackProblem, RotationMode, ShelfOrder};
use crate::solver::Solver;

/// How many orientations a free-rotation item is sampled at. Free rotation
/// has no natural discrete choice, so the packer tries a fixed fan over a
/// half turn (a box repeats after π) and keeps the tidiest.
const FREE_ROTATION_SAMPLES: u32 = 16;

/// Hair of slack left between placed boxes, in metres.
///
/// The packer places boxes exactly flush, but the arrangement is *scored*
/// by SAT over rotated hulls — a different code path, whose rounding puts
/// "exactly touching" a few microns either side of zero. Without this the
/// shelf regularly hands back a layout its own scorer calls infeasible.
/// A tenth of a millimetre is far below anything that matters at world
/// scale and far above the noise.
const PLACEMENT_SLACK: f32 = 1e-4;

/// One placed bounding box.
#[derive(Debug, Clone, Copy)]
struct Placed {
    min: Vec2,
    max: Vec2,
    layers: u32,
}

impl Placed {
    /// Whether this box blocks `other` (overlapping footprints on shared
    /// collision layers).
    fn blocks(
        &self,
        min: Vec2,
        max: Vec2,
        layers: u32,
        layer_rule: crate::problem::LayerRule,
    ) -> bool {
        if !layer_rule.pair_collides(self.layers, layers) {
            return false;
        }
        min.x < self.max.x && max.x > self.min.x && min.y < self.max.y && max.y > self.min.y
    }
}

/// One-shot row packer.
pub struct ShelfSolver {
    layout: Layout,
    done: bool,
}

impl ShelfSolver {
    /// Prepares a run (the placement itself happens on the first step, so
    /// construction stays cheap and the driver owns all the timing).
    pub fn new(problem: &PackProblem) -> Self {
        Self {
            layout: Layout::from_starts(&problem.items),
            done: false,
        }
    }
}

impl Solver for ShelfSolver {
    fn name(&self) -> &'static str {
        "shelf"
    }

    fn layout(&self) -> &Layout {
        &self.layout
    }

    fn is_one_shot(&self) -> bool {
        self.done
    }

    fn step(&mut self, problem: &PackProblem, _scratch: &mut Scratch) {
        self.layout = pack(problem);
        self.done = true;
    }
}

/// The row an item is being fitted into.
#[derive(Debug, Clone, Copy)]
struct Row {
    /// World x of the row's left edge.
    left: f32,
    /// Target row width; an item may only exceed it from the left edge.
    width: f32,
    /// World y of the row's top edge (rows grow downward).
    top: f32,
}

/// A chosen placement.
#[derive(Debug, Clone, Copy)]
struct Fit {
    pose: PosRot,
    min: Vec2,
    max: Vec2,
}

/// The best position and orientation for `item` in `row`, or `None` when it
/// does not fit.
///
/// Candidate x positions are the row's left edge plus just past the right
/// edge of every already-placed box that shares a collision layer with this
/// item — the standard exact candidate set for bottom-left packing. Among
/// the orientations that fit, the leftmost wins, and ties go to the one that
/// leaves the row shortest, which is what stops a single tall outlier from
/// setting the height of a whole row.
fn best_fit(
    problem: &PackProblem,
    item: &crate::problem::PackItem,
    placed: &[Placed],
    row: Row,
    buf: &mut Vec<Vec2>,
) -> Option<Fit> {
    let cfg = &problem.config;
    let mut best: Option<(f32, Fit)> = None;

    for rot in orientations(problem, item.start.rot) {
        place_into(&item.hull, Vec2::ZERO, rot, buf);
        let Some((local_min, local_max)) = bounds(buf) else {
            continue;
        };
        let size = local_max - local_min;

        let mut candidates: Vec<f32> = vec![row.left];
        candidates.extend(placed.iter().filter_map(|p| {
            cfg.layers
                .pair_collides(p.layers, item.layers)
                .then_some(p.max.x + cfg.clearance + PLACEMENT_SLACK)
        }));
        candidates.sort_by(f32::total_cmp);

        for x in candidates {
            if x < row.left - 1e-6 {
                continue;
            }
            // Overflowing the target width is allowed only for an item that
            // is itself wider than the row — otherwise it wraps.
            if x + size.x > row.left + row.width + 1e-6 && x > row.left + 1e-6 {
                continue;
            }
            let min = Vec2::new(x, row.top - size.y);
            let max = min + size;
            if placed
                .iter()
                .any(|p| p.blocks(min, max, item.layers, cfg.layers))
            {
                continue;
            }
            let cost = x.mul_add(1000.0, size.y);
            if best.is_none_or(|(c, _)| cost < c) {
                best = Some((
                    cost,
                    Fit {
                        pose: PosRot {
                            pos: min - local_min,
                            rot,
                        },
                        min,
                        max,
                    },
                ));
            }
            break;
        }
    }
    best.map(|(_, fit)| fit)
}

/// Builds the packed layout.
fn pack(problem: &PackProblem) -> Layout {
    let cfg = &problem.config;
    let mut layout = Layout::from_starts(&problem.items);
    let clearance = cfg.clearance;

    // Pinned items are pre-placed obstacles at their authored positions.
    let mut placed: Vec<Placed> = Vec::new();
    let mut buf: Vec<Vec2> = Vec::new();
    for (i, item) in problem.items.iter().enumerate() {
        if problem.movable(i) {
            continue;
        }
        item.place_into(item.start, &mut buf);
        if let Some((min, max)) = bounds(&buf) {
            placed.push(Placed {
                min,
                max,
                layers: item.layers,
            });
        }
    }
    let has_obstacles = !placed.is_empty();

    let order = placement_order(problem);
    let center = problem.start_center();
    let width = target_width(problem);
    let left = center.x - width * 0.5;
    // Rows grow downward from a top edge chosen so the finished block
    // straddles the selection's centre; a stray estimate is harmless because
    // the whole block is recentred at the end (when nothing is pinned).
    let mut row_top = center.y + estimated_height(problem, width) * 0.5;
    let mut row_bottom = row_top;
    let mut row_started = false;

    for index in order {
        let Some(item) = problem.items.get(index) else {
            continue;
        };
        let row = Row {
            left,
            width,
            top: row_top,
        };
        let fit = best_fit(problem, item, &placed, row, &mut buf);

        // Nothing fit in this row: open a new one under it and place there.
        // The fallback ignores the width limit and every obstacle, which is
        // safe because the new row is below everything already placed — and
        // it is what guarantees the loop always terminates.
        let (pose, min, max) = if let Some(fit) = fit {
            (fit.pose, fit.min, fit.max)
        } else {
            if row_started {
                row_top = row_bottom - clearance - PLACEMENT_SLACK;
            }
            row_started = false;
            let rot = orientations(problem, item.start.rot)
                .into_iter()
                .next()
                .unwrap_or(item.start.rot);
            place_into(&item.hull, Vec2::ZERO, rot, &mut buf);
            let Some((local_min, local_max)) = bounds(&buf) else {
                continue;
            };
            let size = local_max - local_min;
            let min = Vec2::new(left, row_top - size.y);
            (
                PosRot {
                    pos: min - local_min,
                    rot,
                },
                min,
                min + size,
            )
        };

        if !row_started {
            row_started = true;
            row_bottom = min.y;
        }
        row_bottom = row_bottom.min(min.y);
        placed.push(Placed {
            min,
            max,
            layers: item.layers,
        });
        layout.poses[index] = pose;
    }

    if !has_obstacles {
        recenter(problem, &mut layout, center);
    }
    layout
}

/// Slides the whole packed block so its bounding-box centre lands on
/// `center`, keeping the result where the user's selection was.
fn recenter(problem: &PackProblem, layout: &mut Layout, center: Vec2) {
    let mut buf = Vec::new();
    let mut all: Vec<Vec2> = Vec::new();
    for (i, item) in problem.items.iter().enumerate() {
        let Some(pose) = layout.poses.get(i) else {
            continue;
        };
        item.place_into(*pose, &mut buf);
        all.extend_from_slice(&buf);
    }
    let Some((min, max)) = bounds(&all) else {
        return;
    };
    let delta = center - (min + max) * 0.5;
    for pose in &mut layout.poses {
        pose.pos += delta;
    }
}

/// The movable item indices, in placement order.
fn placement_order(problem: &PackProblem) -> Vec<usize> {
    let mut order: Vec<usize> = (0..problem.len()).filter(|i| problem.movable(*i)).collect();
    let params = problem.config.shelf;
    if params.order == ShelfOrder::Selection {
        return order;
    }
    let mut buf = Vec::new();
    // Measure each footprint once, at the item's authored angle — a sort
    // comparator must not depend on a mutable scratch buffer.
    let sizes: Vec<Vec2> = problem
        .items
        .iter()
        .map(|item| item.footprint(item.start.rot, &mut buf))
        .collect();
    let key = |i: &usize| -> f32 {
        let Some(item) = problem.items.get(*i) else {
            return 0.0;
        };
        let size = sizes.get(*i).copied().unwrap_or(Vec2::ZERO);
        match params.order {
            ShelfOrder::Area => item.area,
            ShelfOrder::Height => size.y,
            ShelfOrder::Width => size.x,
            ShelfOrder::Diagonal => size.length(),
            ShelfOrder::Selection => 0.0,
        }
    };
    order.sort_by(|a, b| {
        let (ka, kb) = (key(a), key(b));
        if params.descending {
            kb.total_cmp(&ka).then(a.cmp(b))
        } else {
            ka.total_cmp(&kb).then(a.cmp(b))
        }
    });
    order
}

/// The orientations an item may be placed at, most-preferred first.
fn orientations(problem: &PackProblem, start_rot: f32) -> Vec<f32> {
    let mode = problem.config.rotation;
    if !problem.config.shelf.try_rotations || !mode.allows_rotation() {
        return vec![start_rot];
    }
    let steps = match mode {
        RotationMode::Fixed => return vec![start_rot],
        RotationMode::Quarter => 4,
        RotationMode::Steps { steps } => steps.max(1),
        RotationMode::Free => FREE_ROTATION_SAMPLES,
    };
    let span = if matches!(mode, RotationMode::Free) {
        // A bounding box is symmetric under a half turn, so sampling past π
        // would only repeat work.
        std::f32::consts::PI
    } else {
        std::f32::consts::TAU
    };
    (0..steps)
        .map(|k| start_rot + span * k as f32 / steps as f32)
        .collect()
}

/// The row width the packer aims for.
fn target_width(problem: &PackProblem) -> f32 {
    let total = problem.total_area().max(1e-6);
    match problem.config.boundary {
        // A hard rectangle dictates the row width outright.
        Boundary::Rect { width, .. } => width,
        // For a target aspect ratio r, a block of area A wants
        // w = sqrt(A · r).
        Boundary::Aspect { ratio } => (total * ratio).sqrt(),
        Boundary::Circle { radius } => radius * 2.0_f32.sqrt(),
        // Free: aim square, with room for the packing waste a shelf leaves.
        Boundary::Free => (total / 0.75).sqrt(),
    }
    .max(1e-3)
}

/// A first guess at the finished block height, used only to centre the rows.
fn estimated_height(problem: &PackProblem, width: f32) -> f32 {
    (problem.total_area() / 0.75 / width.max(1e-6)).max(1e-3)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::objective::{Scratch, metrics};
    use crate::problem::{LayerRule, PackConfig, PackItem, SolverKind};
    use crate::solver::PackRun;

    fn rect(center: Vec2, half: Vec2, layers: u32) -> PackItem {
        PackItem::from_world_outline(
            &[
                center + Vec2::new(-half.x, -half.y),
                center + Vec2::new(half.x, -half.y),
                center + Vec2::new(half.x, half.y),
                center + Vec2::new(-half.x, half.y),
            ],
            0.0,
            layers,
            false,
        )
    }

    fn config() -> PackConfig {
        PackConfig {
            solver: SolverKind::Shelf,
            clearance: 0.0,
            rotation: RotationMode::Fixed,
            ..Default::default()
        }
    }

    #[test]
    fn a_shelf_run_finishes_in_one_step() {
        let items = (0..6)
            .map(|i| rect(Vec2::new(i as f32 * 5.0, 0.0), Vec2::splat(0.5), 1))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        assert!(run.step().is_done(), "constructive solvers are one-shot");
        assert_eq!(run.report().total_iterations, 1);
    }

    #[test]
    fn scattered_boxes_come_back_packed_and_legal() {
        let items = (0..9)
            .map(|i| {
                rect(
                    Vec2::new(i as f32 * 7.0, i as f32 * 3.0),
                    Vec2::splat(0.5),
                    1,
                )
            })
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        let report = run.report();
        assert!(report.best.is_feasible(), "shelf results are always legal");
        assert!(
            report.best.fill > 0.6,
            "nine unit squares should pack densely, fill was {:.2}",
            report.best.fill
        );
        assert!(report.shrinkage() > 0.9, "a huge spread must collapse");
    }

    #[test]
    fn mixed_sizes_stay_collision_free() {
        let items = (0..12)
            .map(|i| {
                let s = 0.2 + (i % 4) as f32 * 0.35;
                rect(Vec2::new(i as f32 * 4.0, 0.0), Vec2::new(s, s * 0.6), 1)
            })
            .collect();
        let problem = PackProblem::new(items, config());
        let mut run = PackRun::new(problem);
        run.solve();
        let mut scratch = Scratch::new(run.problem().len());
        let m = metrics(run.problem(), run.best_layout(), &mut scratch);
        assert!(m.is_feasible(), "{:.6} m of overlap left", m.overlap);
    }

    #[test]
    fn clearance_is_left_between_rows_and_columns() {
        let items = (0..8)
            .map(|i| rect(Vec2::new(i as f32 * 4.0, 0.0), Vec2::splat(0.5), 1))
            .collect();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                clearance: 0.3,
                ..config()
            },
        ));
        run.solve();
        let mut scratch = Scratch::new(run.problem().len());
        let m = metrics(run.problem(), run.best_layout(), &mut scratch);
        assert!(
            m.is_feasible(),
            "the 0.3 m gap must be respected everywhere"
        );
    }

    #[test]
    fn off_layer_items_are_allowed_to_share_a_footprint() {
        // Eight items, alternating between two disjoint depth bands. A
        // depth-aware shelf can interleave them, so the result must be
        // meaningfully smaller than the flat packing of the same set.
        let make = || {
            (0..8)
                .map(|i| {
                    rect(
                        Vec2::new(i as f32 * 4.0, 0.0),
                        Vec2::splat(0.5),
                        if i % 2 == 0 { 0b0001 } else { 0b0010 },
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut aware = PackRun::new(PackProblem::new(
            make(),
            PackConfig {
                layers: LayerRule::Respect,
                ..config()
            },
        ));
        aware.solve();
        let mut flat = PackRun::new(PackProblem::new(
            make(),
            PackConfig {
                layers: LayerRule::Solid,
                ..config()
            },
        ));
        flat.solve();
        assert!(
            aware.report().best.extent < flat.report().best.extent * 0.75,
            "depth-aware packing should be much tighter: {:.3} vs flat {:.3}",
            aware.report().best.extent,
            flat.report().best.extent
        );
        assert!(flat.report().best.is_feasible());
    }

    #[test]
    fn pinned_obstacles_are_packed_around_not_over() {
        let mut anchor = rect(Vec2::ZERO, Vec2::splat(1.5), 1);
        anchor.pinned = true;
        let anchor_start = anchor.start;
        let mut items = vec![anchor];
        items.extend(
            (0..6).map(|i| rect(Vec2::new(10.0 + i as f32 * 3.0, 0.0), Vec2::splat(0.5), 1)),
        );
        let mut run = PackRun::new(PackProblem::new(items, config()));
        run.solve();
        assert!(run.best_layout().poses[0].pos.distance(anchor_start.pos) < 1e-6);
        let mut scratch = Scratch::new(run.problem().len());
        assert!(metrics(run.problem(), run.best_layout(), &mut scratch).is_feasible());
    }

    #[test]
    fn quarter_rotations_are_actually_used_and_stay_legal() {
        // Long thin bars against a wide target: the packer should lay some of
        // them down rather than keep every one upright.
        //
        // Note what this does *not* assert. Greedy bottom-left placement is
        // not monotone in the freedom it is given — offering rotations can
        // genuinely produce a worse score than forcing one orientation,
        // because a locally cheaper turn can strand the rest of the row. That
        // is a property of the heuristic, not a bug, and it is exactly why
        // the iterative solvers exist.
        let bars = (0..6)
            .map(|i| rect(Vec2::new(i as f32 * 6.0, 0.0), Vec2::new(0.15, 1.6), 1))
            .collect::<Vec<_>>();
        let starts: Vec<f32> = bars.iter().map(|b| b.start.rot).collect();
        let mut run = PackRun::new(PackProblem::new(
            bars,
            PackConfig {
                boundary: Boundary::Aspect { ratio: 4.0 },
                rotation: RotationMode::Quarter,
                ..config()
            },
        ));
        run.solve();
        let quarter = std::f32::consts::FRAC_PI_2;
        let turned = run
            .best_layout()
            .poses
            .iter()
            .zip(&starts)
            .filter(|(pose, start)| (pose.rot - **start).abs() > 1e-3)
            .count();
        assert!(turned > 0, "no bar was ever laid down");
        for (pose, start) in run.best_layout().poses.iter().zip(&starts) {
            let k = (pose.rot - start) / quarter;
            assert!(
                (k - k.round()).abs() < 1e-4,
                "{} is not a quarter turn",
                pose.rot
            );
        }
        let mut scratch = Scratch::new(run.problem().len());
        assert!(
            metrics(run.problem(), run.best_layout(), &mut scratch).is_feasible(),
            "turning must not create overlaps"
        );
    }

    #[test]
    fn the_target_width_follows_the_boundary() {
        let items = vec![rect(Vec2::ZERO, Vec2::splat(1.0), 1)];
        let boxed = PackProblem::new(
            items.clone(),
            PackConfig {
                boundary: Boundary::Rect {
                    width: 12.0,
                    height: 3.0,
                },
                ..config()
            },
        );
        assert!((target_width(&boxed) - 12.0).abs() < 1e-5);
        // 4 m² of item at a 4:1 target wants a 4 m row.
        let wide = PackProblem::new(
            items,
            PackConfig {
                boundary: Boundary::Aspect { ratio: 4.0 },
                ..config()
            },
        );
        assert!((target_width(&wide) - 4.0).abs() < 1e-4);
    }

    #[test]
    fn selection_order_places_items_left_to_right_as_clicked() {
        let items = vec![
            rect(Vec2::new(0.0, 0.0), Vec2::splat(0.2), 1),
            rect(Vec2::new(9.0, 0.0), Vec2::splat(0.8), 1),
            rect(Vec2::new(18.0, 0.0), Vec2::splat(0.5), 1),
        ];
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                // A wide target keeps all three in one row, so "left to
                // right" is about ordering rather than about wrapping.
                boundary: Boundary::Aspect { ratio: 20.0 },
                shelf: crate::problem::ShelfParams {
                    order: ShelfOrder::Selection,
                    ..Default::default()
                },
                ..config()
            },
        ));
        run.solve();
        let p = &run.best_layout().poses;
        assert!(p[0].pos.x < p[1].pos.x && p[1].pos.x < p[2].pos.x);
    }

    #[test]
    fn a_wide_target_produces_a_wide_block() {
        let items = (0..16)
            .map(|i| rect(Vec2::new(i as f32 * 3.0, 0.0), Vec2::splat(0.5), 1))
            .collect();
        let mut run = PackRun::new(PackProblem::new(
            items,
            PackConfig {
                boundary: Boundary::Aspect { ratio: 4.0 },
                ..config()
            },
        ));
        run.solve();
        let (min, max) = run.report().best.bounds;
        let size = max - min;
        assert!(
            size.x > size.y * 2.0,
            "asked for 4:1, got {:.2}×{:.2}",
            size.x,
            size.y
        );
    }
}
