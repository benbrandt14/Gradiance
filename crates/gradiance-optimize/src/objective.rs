//! Scoring a candidate arrangement.
//!
//! Everything a solver needs to compare two layouts goes through
//! [`metrics`]. The score is a **weighted sum of dimensionless terms**
//! ([`ObjectiveWeights`]) rather than one number, which is what lets the
//! same machinery express "pack this as tight as possible", "line these up",
//! and "spread these out inside a box" without a different solver for each.
//!
//! # Why the terms are normalized
//!
//! Every term is scaled to be roughly `0..1` against a **reference length**
//! derived from the total item area — a quantity that does not change during
//! a run. Two consequences, both load-bearing:
//!
//! - A weight means the same thing whether the selection is a 10 cm cluster
//!   or a 40 m field. Raw areas would make every weight scene-specific and
//!   therefore useless as a saved setting.
//! - The reference is *constant*, so the solvers are minimizing a fixed
//!   function. Normalizing by the current extent instead (the obvious
//!   alternative) makes the objective move as the layout does, and gradient
//!   methods chase their own tail.
//!
//! # The gap term
//!
//! The single most useful term is not the bounding box — it is
//! [`ObjectiveWeights::gap`], the mean leftover space between *neighbouring*
//! bodies. A body in the middle of a cluster contributes nothing to the
//! bounding box, so a pure extent objective gives it **exactly zero
//! gradient** and it simply never tightens. The gap term is local, so every
//! body always knows which way is denser. That is what closes the last few
//! percent of fill ratio.

use bevy::math::Vec2;
use gradiance_core::units::PosRot;

use crate::hull::{bounds, convex_hull, polygon_area, polygon_perimeter};
use crate::problem::{Boundary, EdgeAlignment, Layout, Objective, PackProblem};
use crate::sat::{contact_span, separation};

/// Reusable placement buffers — one world-space hull per item.
#[derive(Debug, Default, Clone)]
pub struct Scratch {
    placed: Vec<Vec<Vec2>>,
    centers: Vec<Vec2>,
    verts: Vec<Vec2>,
}

impl Scratch {
    /// Buffers sized for `n` items.
    pub fn new(n: usize) -> Self {
        Self {
            placed: vec![Vec::new(); n],
            centers: vec![Vec2::ZERO; n],
            verts: Vec::new(),
        }
    }

    /// Re-places every item at `layout`, reusing the existing allocations.
    pub fn refresh(&mut self, problem: &PackProblem, layout: &Layout) {
        self.placed.resize(problem.items.len(), Vec::new());
        self.centers.resize(problem.items.len(), Vec2::ZERO);
        self.verts.clear();
        for (i, item) in problem.items.iter().enumerate() {
            let pose = layout.poses.get(i).copied().unwrap_or(item.start);
            item.place_into(pose, &mut self.placed[i]);
            self.centers[i] = pose.pos;
            self.verts.extend_from_slice(&self.placed[i]);
        }
    }

    /// The world-space hull of item `i` as of the last [`refresh`](Self::refresh).
    pub fn placed(&self, i: usize) -> &[Vec2] {
        self.placed.get(i).map_or(&[], Vec::as_slice)
    }

    /// The placed centre of item `i`.
    pub fn center(&self, i: usize) -> Vec2 {
        self.centers.get(i).copied().unwrap_or(Vec2::ZERO)
    }

    /// How many items are placed.
    pub fn len(&self) -> usize {
        self.placed.len()
    }

    /// Whether nothing is placed.
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }

    /// Every placed vertex, in item order.
    pub fn vertices(&self) -> &[Vec2] {
        &self.verts
    }
}

/// The full score of one layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// The weighted, dimensionless scalar the solvers minimize.
    pub objective: f32,
    /// The pure geometric measure selected by [`Objective`] — the number to
    /// show a user, since it is in real units (m² or m) and does not move
    /// when weights do.
    pub extent: f32,
    /// Axis-aligned bounds of the whole arrangement.
    pub bounds: (Vec2, Vec2),
    /// Sum of every pair's missing clearance, in metres.
    pub overlap: f32,
    /// How many pairs are still violating.
    pub violations: u32,
    /// Total item area over the extent measure's own area — the packing
    /// density, and the headline quality number. 1.0 is a perfect tiling.
    pub fill: f32,
    /// Total item area over convex-hull area, regardless of which objective
    /// is selected (so the readout is comparable across objectives).
    pub hull_fill: f32,
    /// Smallest gap between any neighbouring pair, in metres. Negative means
    /// something is still overlapping.
    pub min_gap: f32,
    /// Mean leftover gap over neighbouring pairs, in metres — how much slack
    /// is left to squeeze out.
    pub mean_gap: f32,
    /// Total facing length between touching pairs, in metres.
    pub contact: f32,
    /// Edge-direction alignment, `0..1`. 1 means every hull edge in the
    /// arrangement is parallel (or axis-aligned, per [`EdgeAlignment`]).
    pub alignment: f32,
    /// How far outside a hard boundary the arrangement reaches, in metres.
    pub boundary_error: f32,
}

impl Metrics {
    /// Residual overlap, in metres, that still counts as collision-free.
    ///
    /// Feasibility cannot be a test against exact zero. The solvers place
    /// bodies by one code path (rotating hulls, accumulating corrections)
    /// and score them by another (SAT projections), and the two disagree in
    /// the last few bits — a shelf packing that placed two boxes perfectly
    /// flush scores a ~3 µm "penetration". Testing `> 0.0` therefore rejects
    /// good layouts as illegal, which is worse in every way. Ten microns is
    /// four orders of magnitude below the default clearance and three above
    /// the noise.
    pub const PENETRATION_TOLERANCE: f32 = 1e-5;

    /// Slack allowed against a hard boundary, in metres.
    pub const BOUNDARY_TOLERANCE: f32 = 1e-4;

    /// Whether the layout satisfies every hard constraint.
    pub fn is_feasible(&self) -> bool {
        self.overlap <= Self::PENETRATION_TOLERANCE
            && self.boundary_error <= Self::BOUNDARY_TOLERANCE
    }
}

/// What one pass over the pairs found.
#[derive(Debug, Default, Clone, Copy)]
struct PairStats {
    overlap: f32,
    violations: u32,
    /// Sum of positive gaps over neighbouring pairs.
    gap_sum: f32,
    gap_count: u32,
    min_gap: f32,
    contact: f32,
}

/// Scores `layout`, refreshing `scratch` in the process.
pub fn metrics(problem: &PackProblem, layout: &Layout, scratch: &mut Scratch) -> Metrics {
    scratch.refresh(problem, layout);
    measure(problem, scratch)
}

/// Scores whatever is already placed in `scratch`.
///
/// Split out because the gradient evaluator re-places only the items it
/// perturbed; re-placing all of them per finite difference would dominate
/// the cost.
pub fn measure(problem: &PackProblem, scratch: &Scratch) -> Metrics {
    let (min, max) = bounds(scratch.vertices()).unwrap_or((Vec2::ZERO, Vec2::ZERO));
    let size = (max - min).max(Vec2::ZERO);
    let cfg = &problem.config;
    let total_area = problem.total_area();

    // The convex hull is needed by two objectives and always by the hull-fill
    // readout, but it is O(V log V) and the gradient evaluator calls this
    // thousands of times — so compute it exactly once, here.
    let hull = convex_hull(scratch.vertices());
    let hull_area = polygon_area(&hull);

    let extent = match cfg.objective {
        Objective::BoundingArea => size.x * size.y,
        Objective::HullArea => hull_area,
        Objective::EnclosingCircle => {
            let c = (min + max) * 0.5;
            let r = scratch
                .vertices()
                .iter()
                .map(|v| v.distance(c))
                .fold(0.0_f32, f32::max);
            std::f32::consts::PI * r * r
        }
        Objective::HullPerimeter => polygon_perimeter(&hull),
    };

    let pairs = pair_stats(problem, scratch);
    let boundary_error = boundary_error(problem, scratch, min, max);
    let alignment = edge_alignment(scratch, cfg.alignment);

    // Fill against whichever area the objective is actually shrinking.
    let fill_area = match cfg.objective {
        Objective::BoundingArea => size.x * size.y,
        Objective::HullArea | Objective::HullPerimeter => hull_area,
        Objective::EnclosingCircle => extent,
    };
    let fill = ratio(total_area, fill_area);
    let hull_fill = ratio(total_area, hull_area);

    // Reference scales: fixed for the whole run (they depend only on the
    // items, not the layout), so the objective is a stationary function.
    let area_ref = total_area.max(1e-9);
    let length_ref = area_ref.sqrt();

    let extent_term = match cfg.objective {
        Objective::HullPerimeter => extent / (4.0 * length_ref),
        _ => extent / area_ref,
    };
    let w = cfg.weights;
    let mean_gap = if pairs.gap_count > 0 {
        pairs.gap_sum / pairs.gap_count as f32
    } else {
        0.0
    };

    let objective = w.extent * extent_term
        + w.fill * (1.0 - fill).max(0.0)
        + w.gap * (mean_gap / length_ref)
        + w.parallel * (1.0 - alignment)
        + w.contact * (pairs.contact / length_ref)
        // Hard constraints are not weights — they are always paid, and paid
        // heavily enough that no amount of tidiness buys an illegal layout.
        + (pairs.overlap / length_ref) * cfg.anneal.overlap_penalty.max(1.0)
        + (boundary_error / length_ref) * 100.0
        + aspect_penalty(problem, size);

    Metrics {
        objective,
        extent,
        bounds: (min, max),
        overlap: pairs.overlap,
        violations: pairs.violations,
        fill,
        hull_fill,
        min_gap: if pairs.gap_count > 0 {
            pairs.min_gap
        } else {
            0.0
        },
        mean_gap,
        contact: pairs.contact,
        alignment,
        boundary_error,
    }
}

/// `numerator / denominator`, guarding a degenerate denominator.
fn ratio(numerator: f32, denominator: f32) -> f32 {
    if denominator > 1e-9 {
        numerator / denominator
    } else {
        0.0
    }
}

/// One pass over every pair that could matter: overlap, gaps, contact.
fn pair_stats(problem: &PackProblem, scratch: &Scratch) -> PairStats {
    let n = problem.items.len();
    let cfg = &problem.config;
    let clearance = cfg.clearance;
    // Only *neighbours* contribute a gap. Without a cutoff the gap term
    // degenerates into "everything attracts everything", which is precisely
    // the naive force packing this crate exists to beat.
    let reach = problem.mean_radius() * cfg.neighborhood.max(0.0);
    let mut stats = PairStats {
        min_gap: f32::MAX,
        ..PairStats::default()
    };

    // The gap term reads a fixed *count* of nearest neighbours, never a
    // radius — see `PackConfig::gap_neighbors` for why a radius is gameable.
    let centers: Vec<Vec2> = (0..n).map(|i| scratch.center(i)).collect();
    let gap_pairs = problem.nearest_pairs(&centers, cfg.gap_neighbors as usize);

    for i in 0..n {
        for j in (i + 1)..n {
            if !problem.pair_collides(i, j) {
                continue;
            }
            // Two pinned items are not the solver's problem — counting them
            // would make an unreachable score floor look like failure.
            if !problem.movable(i) && !problem.movable(j) {
                continue;
            }
            let (Some(a), Some(b)) = (problem.items.get(i), problem.items.get(j)) else {
                continue;
            };
            let broad = a.radius + b.radius + clearance + reach;
            if scratch.center(i).distance_squared(scratch.center(j)) > broad * broad {
                continue;
            }
            let Some(sep) = separation(scratch.placed(i), scratch.placed(j)) else {
                continue;
            };

            if sep.distance < clearance {
                stats.overlap += clearance - sep.distance;
                stats.violations += 1;
            }
            // "Touching" for the contact term means within a hair of the
            // requested clearance, not literally zero — bodies packed to a
            // 2 cm gap are in contact for every purpose the term serves.
            if sep.distance <= clearance + Metrics::PENETRATION_TOLERANCE.max(clearance * 0.1) {
                stats.contact += contact_span(scratch.placed(i), scratch.placed(j), sep.axis);
            }
        }
    }

    for (i, j) in gap_pairs {
        let Some(sep) = separation(scratch.placed(i), scratch.placed(j)) else {
            continue;
        };
        stats.gap_sum += (sep.distance - clearance).max(0.0);
        stats.gap_count += 1;
        stats.min_gap = stats.min_gap.min(sep.distance);
    }
    stats
}

/// How aligned the arrangement's hull edges are, in `0..1`.
///
/// Uses the circular concentration of edge directions folded by
/// [`EdgeAlignment`]: each edge contributes `e^{i·k·θ}` weighted by its own
/// length (a 2 m edge matters more than a 2 cm one), and the result is the
/// magnitude of the mean. Folding by `k` is what makes the measure
/// direction-agnostic — with `k = 2` a segment and its reverse agree, and
/// with `k = 4` so do perpendicular edges, which is the version that makes
/// rectangles settle into a grid rather than a diagonal stack.
fn edge_alignment(scratch: &Scratch, mode: EdgeAlignment) -> f32 {
    let k = mode.fold() as f32;
    let mut acc = Vec2::ZERO;
    let mut total = 0.0;
    for i in 0..scratch.placed.len() {
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
            let angle = edge.y.atan2(edge.x) * k;
            let (sin, cos) = angle.sin_cos();
            acc += Vec2::new(cos, sin) * len;
            total += len;
        }
    }
    if total <= 1e-9 {
        return 1.0;
    }
    (acc.length() / total).clamp(0.0, 1.0)
}

/// Soft penalty for departing from a target aspect ratio (zero for every
/// other boundary mode).
fn aspect_penalty(problem: &PackProblem, size: Vec2) -> f32 {
    let Boundary::Aspect { ratio } = problem.config.boundary else {
        return 0.0;
    };
    if size.x <= 1e-6 || size.y <= 1e-6 {
        return 0.0;
    }
    // Log-ratio error is symmetric: twice as wide costs the same as twice as
    // tall, which a plain difference would not give.
    ((size.x / size.y) / ratio).ln().abs()
}

/// How far the arrangement pokes out of a hard boundary, in metres.
fn boundary_error(problem: &PackProblem, scratch: &Scratch, min: Vec2, max: Vec2) -> f32 {
    match problem.config.boundary {
        Boundary::Free | Boundary::Aspect { .. } => 0.0,
        Boundary::Rect { width, height } => {
            let c = problem.start_center();
            let half = Vec2::new(width, height) * 0.5;
            let low = (c - half) - min;
            let high = max - (c + half);
            low.max(Vec2::ZERO).length() + high.max(Vec2::ZERO).length()
        }
        Boundary::Circle { radius } => {
            let c = problem.start_center();
            scratch
                .vertices()
                .iter()
                .map(|v| (v.distance(c) - radius).max(0.0))
                .fold(0.0_f32, f32::max)
        }
    }
}

/// Projects `pose` back inside a hard boundary, given the item's circum-
/// radius. Returns the corrected position (unchanged for soft boundaries).
///
/// Solvers call this after every move so a hard container is enforced by
/// construction rather than only paid for in the score.
pub fn clamp_to_boundary(problem: &PackProblem, pose: PosRot, radius: f32) -> Vec2 {
    match problem.config.boundary {
        Boundary::Free | Boundary::Aspect { .. } => pose.pos,
        Boundary::Rect { width, height } => {
            let c = problem.start_center();
            // A container smaller than the item cannot be satisfied; centring
            // is the least-wrong answer and keeps the solver stable.
            let half = (Vec2::new(width, height) * 0.5 - Vec2::splat(radius)).max(Vec2::ZERO);
            (pose.pos - c).clamp(-half, half) + c
        }
        Boundary::Circle { radius: r } => {
            let c = problem.start_center();
            let limit = (r - radius).max(0.0);
            let d = pose.pos - c;
            if d.length() > limit {
                c + d.normalize_or_zero() * limit
            } else {
                pose.pos
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::{LayerRule, PackConfig, PackItem};

    fn square_item(center: Vec2, half: f32, layers: u32) -> PackItem {
        PackItem::from_world_outline(
            &[
                center + Vec2::new(-half, -half),
                center + Vec2::new(half, -half),
                center + Vec2::new(half, half),
                center + Vec2::new(-half, half),
            ],
            0.0,
            layers,
            false,
        )
    }

    fn problem(items: Vec<PackItem>, config: PackConfig) -> PackProblem {
        PackProblem::new(items, config)
    }

    fn score(p: &PackProblem) -> Metrics {
        metrics(
            p,
            &Layout::from_starts(&p.items),
            &mut Scratch::new(p.len()),
        )
    }

    #[test]
    fn two_touching_unit_squares_measure_the_expected_box_and_fill() {
        let p = problem(
            vec![
                square_item(Vec2::new(-0.5, 0.0), 0.5, 1),
                square_item(Vec2::new(0.5, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        let m = score(&p);
        assert!((m.extent - 2.0).abs() < 1e-4, "2×1 box");
        assert!((m.fill - 1.0).abs() < 1e-4, "perfectly packed");
        assert!((m.hull_fill - 1.0).abs() < 1e-4);
        assert_eq!(m.violations, 0);
        assert!(m.is_feasible());
    }

    #[test]
    fn overlap_is_counted_and_makes_the_layout_infeasible() {
        let p = problem(
            vec![
                square_item(Vec2::ZERO, 0.5, 1),
                square_item(Vec2::new(0.4, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        let m = score(&p);
        assert_eq!(m.violations, 1);
        assert!((m.overlap - 0.6).abs() < 1e-4, "1.0 wide, 0.4 apart");
        assert!(!m.is_feasible());
    }

    #[test]
    fn disjoint_depth_layers_may_share_a_footprint() {
        let p = problem(
            vec![
                square_item(Vec2::ZERO, 0.5, 0b0001),
                square_item(Vec2::ZERO, 0.5, 0b0010),
            ],
            PackConfig {
                layers: LayerRule::Respect,
                ..Default::default()
            },
        );
        assert_eq!(
            score(&p).violations,
            0,
            "different depth bands never collide"
        );

        let flat = problem(
            vec![
                square_item(Vec2::ZERO, 0.5, 0b0001),
                square_item(Vec2::ZERO, 0.5, 0b0010),
            ],
            PackConfig {
                layers: LayerRule::Solid,
                ..Default::default()
            },
        );
        assert_eq!(
            score(&flat).violations,
            1,
            "flat mode makes them solid again"
        );
    }

    #[test]
    fn clearance_widens_what_counts_as_a_violation() {
        let p = problem(
            vec![
                square_item(Vec2::new(-0.6, 0.0), 0.5, 1),
                square_item(Vec2::new(0.6, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.5,
                ..Default::default()
            },
        );
        let m = score(&p);
        assert_eq!(m.violations, 1, "0.2 apart violates a 0.5 clearance");
        assert!((m.overlap - 0.3).abs() < 1e-4);
    }

    #[test]
    fn the_hull_objective_sees_through_a_diagonal_bounding_box() {
        let items = vec![
            square_item(Vec2::ZERO, 0.5, 1),
            square_item(Vec2::new(3.0, 3.0), 0.5, 1),
        ];
        let boxed = problem(
            items.clone(),
            PackConfig {
                objective: Objective::BoundingArea,
                ..Default::default()
            },
        );
        let hulled = problem(
            items,
            PackConfig {
                objective: Objective::HullArea,
                ..Default::default()
            },
        );
        assert!(
            score(&boxed).extent > score(&hulled).extent,
            "the hull is tighter than the box"
        );
    }

    #[test]
    fn gaps_are_measured_between_neighbours() {
        // Two unit squares with a 0.5 m face-to-face gap.
        let p = problem(
            vec![
                square_item(Vec2::new(-0.75, 0.0), 0.5, 1),
                square_item(Vec2::new(0.75, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        let m = score(&p);
        assert!((m.min_gap - 0.5).abs() < 1e-4, "min gap was {}", m.min_gap);
        assert!((m.mean_gap - 0.5).abs() < 1e-4);
    }

    /// However far apart bodies are, they still have nearest neighbours, so
    /// the gap term still reads a cost.
    ///
    /// This is the anti-gaming property. Scoring the gap over "pairs within
    /// radius R" instead lets a solver empty the set by spreading out, at
    /// which point exploding the arrangement is a global minimum — which is
    /// exactly what happened before the term was changed to a fixed count of
    /// nearest neighbours.
    #[test]
    fn far_apart_bodies_still_have_neighbours_and_still_cost() {
        let far = problem(
            vec![
                square_item(Vec2::new(-40.0, 0.0), 0.5, 1),
                square_item(Vec2::new(40.0, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        let near = problem(
            vec![
                square_item(Vec2::new(-0.75, 0.0), 0.5, 1),
                square_item(Vec2::new(0.75, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        assert!(score(&far).mean_gap > score(&near).mean_gap);
        assert!(score(&far).mean_gap > 70.0, "the whole 79 m gap is counted");
    }

    #[test]
    fn contact_span_is_the_shared_face_length() {
        // Two unit squares face to face: one full metre of contact.
        let p = problem(
            vec![
                square_item(Vec2::new(-0.5, 0.0), 0.5, 1),
                square_item(Vec2::new(0.5, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        assert!((score(&p).contact - 1.0).abs() < 1e-3);

        // Offset so only half the faces overlap: half the contact.
        let half = problem(
            vec![
                square_item(Vec2::new(-0.5, 0.0), 0.5, 1),
                square_item(Vec2::new(0.5, 0.5), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        assert!((score(&half).contact - 0.5).abs() < 1e-3);
    }

    #[test]
    fn alignment_is_one_for_a_grid_and_falls_for_a_rotated_member() {
        let aligned = problem(
            vec![
                square_item(Vec2::new(-1.0, 0.0), 0.5, 1),
                square_item(Vec2::new(1.0, 0.0), 0.5, 1),
            ],
            PackConfig::default(),
        );
        assert!(
            score(&aligned).alignment > 0.999,
            "two axis-aligned squares are perfectly aligned"
        );

        let mut tilted_items = vec![square_item(Vec2::new(-1.0, 0.0), 0.5, 1)];
        tilted_items.push(PackItem::from_world_outline(
            &[
                Vec2::new(1.0, 0.0),
                Vec2::new(1.5, 0.5),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.5, 0.5),
            ],
            std::f32::consts::FRAC_PI_4,
            1,
            false,
        ));
        let tilted = problem(tilted_items, PackConfig::default());
        assert!(
            score(&tilted).alignment < 0.6,
            "a 45° member breaks the grid"
        );
    }

    #[test]
    fn parallel_and_orthogonal_folding_disagree_about_a_quarter_turn() {
        // A square rotated 90° is identical under orthogonal folding and
        // still identical under parallel folding (its edges come in
        // perpendicular pairs), so use a long bar, where the distinction is
        // real: turned 90° it is anti-parallel but still axis-aligned.
        let bar = |rot: f32, at: Vec2| {
            PackItem::from_world_outline(
                &[
                    at + Vec2::new(-1.0, -0.1),
                    at + Vec2::new(1.0, -0.1),
                    at + Vec2::new(1.0, 0.1),
                    at + Vec2::new(-1.0, 0.1),
                ],
                rot,
                1,
                false,
            )
        };
        let quarter = std::f32::consts::FRAC_PI_2;
        let mixed = vec![
            bar(0.0, Vec2::new(-3.0, 0.0)),
            bar(quarter, Vec2::new(3.0, 0.0)),
        ];

        let parallel = problem(
            mixed.clone(),
            PackConfig {
                alignment: EdgeAlignment::Parallel,
                ..Default::default()
            },
        );
        let orthogonal = problem(
            mixed,
            PackConfig {
                alignment: EdgeAlignment::Orthogonal,
                ..Default::default()
            },
        );
        assert!(
            score(&orthogonal).alignment > score(&parallel).alignment,
            "a quarter turn is still axis-aligned but is not parallel"
        );
    }

    #[test]
    fn a_hard_rectangle_reports_and_clamps_the_overflow() {
        let p = problem(
            vec![
                square_item(Vec2::new(-5.0, 0.0), 0.5, 1),
                square_item(Vec2::new(5.0, 0.0), 0.5, 1),
            ],
            PackConfig {
                boundary: Boundary::Rect {
                    width: 4.0,
                    height: 4.0,
                },
                ..Default::default()
            },
        );
        let m = score(&p);
        assert!(m.boundary_error > 0.0);
        assert!(!m.is_feasible());

        let clamped = clamp_to_boundary(&p, p.items[1].start, p.items[1].radius);
        assert!(clamped.x <= 2.0 + 1e-4, "inside the half-width");
    }

    #[test]
    fn a_hard_circle_clamps_to_its_radius() {
        // The boundary is centred on the *arrangement*, so it takes at least
        // two items for anything to be outside it.
        let p = problem(
            vec![
                square_item(Vec2::new(-9.0, 0.0), 0.5, 1),
                square_item(Vec2::new(9.0, 0.0), 0.5, 1),
            ],
            PackConfig {
                boundary: Boundary::Circle { radius: 3.0 },
                ..Default::default()
            },
        );
        let center = p.start_center();
        let clamped = clamp_to_boundary(&p, p.items[1].start, p.items[1].radius);
        assert!(clamped.distance(center) <= 3.0 + 1e-4);
    }

    #[test]
    fn the_aspect_penalty_is_symmetric_in_the_log() {
        let wide = problem(
            vec![
                square_item(Vec2::new(-1.0, 0.0), 0.5, 1),
                square_item(Vec2::new(1.0, 0.0), 0.5, 1),
            ],
            PackConfig {
                boundary: Boundary::Aspect { ratio: 1.0 },
                ..Default::default()
            },
        );
        let tall = problem(
            vec![
                square_item(Vec2::new(0.0, -1.0), 0.5, 1),
                square_item(Vec2::new(0.0, 1.0), 0.5, 1),
            ],
            PackConfig {
                boundary: Boundary::Aspect { ratio: 1.0 },
                ..Default::default()
            },
        );
        assert!(
            (score(&wide).objective - score(&tall).objective).abs() < 1e-3,
            "3:1 wide and 3:1 tall must cost the same against a 1:1 target"
        );
    }

    #[test]
    fn two_pinned_items_do_not_count_as_a_solvable_violation() {
        let mut a = square_item(Vec2::ZERO, 0.5, 1);
        let mut b = square_item(Vec2::new(0.2, 0.0), 0.5, 1);
        a.pinned = true;
        b.pinned = true;
        let p = problem(vec![a, b], PackConfig::default());
        assert_eq!(
            score(&p).violations,
            0,
            "nothing the solver could do about it"
        );
    }

    #[test]
    fn the_score_is_scale_free() {
        // The same arrangement at 1 m and at 100 m must score identically:
        // weights are saved settings, so they cannot mean different things
        // in a tabletop scene and a landscape one.
        let build = |s: f32| {
            problem(
                vec![
                    square_item(Vec2::new(-0.75 * s, 0.0), 0.5 * s, 1),
                    square_item(Vec2::new(0.75 * s, 0.0), 0.5 * s, 1),
                ],
                PackConfig {
                    clearance: 0.0,
                    ..Default::default()
                },
            )
        };
        let small = score(&build(1.0)).objective;
        let large = score(&build(100.0)).objective;
        assert!(
            (small - large).abs() < 1e-3,
            "scale changed the score: {small} vs {large}"
        );
    }

    #[test]
    fn a_denser_layout_always_scores_better_under_the_default_weights() {
        let spread = problem(
            vec![
                square_item(Vec2::new(-3.0, 0.0), 0.5, 1),
                square_item(Vec2::new(3.0, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        let tight = problem(
            vec![
                square_item(Vec2::new(-0.5, 0.0), 0.5, 1),
                square_item(Vec2::new(0.5, 0.0), 0.5, 1),
            ],
            PackConfig {
                clearance: 0.0,
                ..Default::default()
            },
        );
        assert!(score(&tight).objective < score(&spread).objective);
    }
}
