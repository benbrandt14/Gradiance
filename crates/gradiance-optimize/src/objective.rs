//! Scoring a candidate arrangement.
//!
//! Everything a solver needs to compare two layouts goes through
//! [`metrics`]: the geometric extent it is minimizing, the residual overlap,
//! and the boundary error, combined into one scalar
//! ([`Metrics::objective`]). Keeping the combination in one place is what
//! lets three very different searches share a definition of "better", and
//! lets the UI show the *same* number the solver is driving down.
//!
//! Placement is cached in a [`Scratch`] buffer: the O(n²) overlap scan runs
//! every iteration, and re-rotating every hull inside it would dominate the
//! cost.

use bevy::math::Vec2;
use gradiance_core::units::PosRot;

use crate::hull::{bounds, convex_hull, polygon_area, polygon_perimeter};
use crate::problem::{Boundary, Layout, Objective, PackProblem};
use crate::sat::penetration;

/// Reusable placement buffers — one world-space hull per item.
#[derive(Debug, Default, Clone)]
pub struct Scratch {
    placed: Vec<Vec<Vec2>>,
    centers: Vec<Vec2>,
}

impl Scratch {
    /// Buffers sized for `n` items.
    pub fn new(n: usize) -> Self {
        Self {
            placed: vec![Vec::new(); n],
            centers: vec![Vec2::ZERO; n],
        }
    }

    /// Re-places every item at `layout`, reusing the existing allocations.
    pub fn refresh(&mut self, problem: &PackProblem, layout: &Layout) {
        self.placed.resize(problem.items.len(), Vec::new());
        self.centers.resize(problem.items.len(), Vec2::ZERO);
        for (i, item) in problem.items.iter().enumerate() {
            let pose = layout.poses.get(i).copied().unwrap_or(item.start);
            item.place_into(pose, &mut self.placed[i]);
            self.centers[i] = pose.pos;
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

    /// Every placed vertex, in item order.
    pub fn vertices(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.placed.iter().flat_map(|p| p.iter().copied())
    }
}

/// The full score of one layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// The scalar the solvers minimize: [`extent`](Self::extent) plus the
    /// overlap and boundary penalties.
    pub objective: f32,
    /// The pure geometric measure selected by [`Objective`] — the number to
    /// show a user, since it is in real units (m² or m) and does not move
    /// when penalties do.
    pub extent: f32,
    /// Axis-aligned bounds of the whole arrangement.
    pub bounds: (Vec2, Vec2),
    /// Sum of every pair's missing clearance, in metres. Zero means the
    /// layout is legal.
    pub overlap: f32,
    /// How many pairs are still violating.
    pub violations: u32,
    /// Total item area over bounding-box area — the packing density, and the
    /// most legible progress readout.
    pub fill: f32,
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
    /// good layouts as illegal, which is worse than the alternative in every
    /// way. Ten microns is four orders of magnitude below the default
    /// clearance and three above the noise.
    pub const PENETRATION_TOLERANCE: f32 = 1e-5;

    /// Slack allowed against a hard boundary, in metres.
    pub const BOUNDARY_TOLERANCE: f32 = 1e-4;

    /// Whether the layout satisfies every hard constraint.
    pub fn is_feasible(&self) -> bool {
        self.overlap <= Self::PENETRATION_TOLERANCE
            && self.boundary_error <= Self::BOUNDARY_TOLERANCE
    }
}

/// Scores `layout`, refreshing `scratch` in the process.
pub fn metrics(problem: &PackProblem, layout: &Layout, scratch: &mut Scratch) -> Metrics {
    scratch.refresh(problem, layout);
    let n = problem.items.len();

    let (min, max) = bounds_of(scratch).unwrap_or((Vec2::ZERO, Vec2::ZERO));
    let size = (max - min).max(Vec2::ZERO);

    let extent = match problem.config.objective {
        Objective::BoundingArea => size.x * size.y,
        Objective::HullArea => polygon_area(&convex_hull(&scratch.vertices().collect::<Vec<_>>())),
        Objective::EnclosingCircle => {
            let c = (min + max) * 0.5;
            let r = scratch
                .vertices()
                .map(|v| v.distance(c))
                .fold(0.0_f32, f32::max);
            std::f32::consts::PI * r * r
        }
        Objective::HullPerimeter => {
            polygon_perimeter(&convex_hull(&scratch.vertices().collect::<Vec<_>>()))
        }
    };

    // Residual overlap, broad-phased on circumradius.
    let mut overlap = 0.0;
    let mut violations = 0u32;
    let clearance = problem.config.clearance;
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
            let reach = a.radius + b.radius + clearance;
            if scratch.center(i).distance_squared(scratch.center(j)) > reach * reach {
                continue;
            }
            if let Some(mtv) = penetration(scratch.placed(i), scratch.placed(j), clearance) {
                overlap += mtv.depth;
                violations += 1;
            }
        }
    }

    let boundary_error = boundary_error(problem, scratch, min, max);

    // Penalty weights are relative to the extent so they stay meaningful
    // whether the selection is centimetres or tens of metres across.
    let scale = extent.max(1e-6);
    let objective = extent
        + overlap * scale * problem.config.anneal.overlap_penalty.max(1.0)
        + boundary_error * scale * 10.0
        + aspect_penalty(problem, size) * scale;

    let bounding_area = size.x * size.y;
    let fill = if bounding_area > 1e-9 {
        problem.total_area() / bounding_area
    } else {
        0.0
    };

    Metrics {
        objective,
        extent,
        bounds: (min, max),
        overlap,
        violations,
        fill,
        boundary_error,
    }
}

/// Bounds over every placed vertex.
fn bounds_of(scratch: &Scratch) -> Option<(Vec2, Vec2)> {
    let verts: Vec<Vec2> = scratch.vertices().collect();
    bounds(&verts)
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
        let m = metrics(&p, &Layout::from_starts(&p.items), &mut Scratch::new(2));
        assert!((m.extent - 2.0).abs() < 1e-4, "2×1 box");
        assert!((m.fill - 1.0).abs() < 1e-4, "perfectly packed");
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
        let m = metrics(&p, &Layout::from_starts(&p.items), &mut Scratch::new(2));
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
        let m = metrics(&p, &Layout::from_starts(&p.items), &mut Scratch::new(2));
        assert_eq!(m.violations, 0, "different depth bands never collide");

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
        let m = metrics(
            &flat,
            &Layout::from_starts(&flat.items),
            &mut Scratch::new(2),
        );
        assert_eq!(m.violations, 1, "flat mode makes them solid again");
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
        let m = metrics(&p, &Layout::from_starts(&p.items), &mut Scratch::new(2));
        assert_eq!(m.violations, 1, "0.2 apart violates a 0.5 clearance");
        assert!((m.overlap - 0.3).abs() < 1e-4);
    }

    #[test]
    fn the_hull_objective_sees_through_a_diagonal_bounding_box() {
        // Two squares on a diagonal: the bounding box is much larger than the
        // hull, so the two objectives must disagree.
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
        let mut s = Scratch::new(2);
        let a = metrics(&boxed, &Layout::from_starts(&boxed.items), &mut s);
        let b = metrics(&hulled, &Layout::from_starts(&hulled.items), &mut s);
        assert!(a.extent > b.extent, "the hull is tighter than the box");
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
        let m = metrics(&p, &Layout::from_starts(&p.items), &mut Scratch::new(2));
        assert!(m.boundary_error > 0.0);
        assert!(!m.is_feasible());

        // Clamping puts the item back inside, allowing for its own radius.
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
        assert!(center.length() < 1e-4, "the pair straddles the origin");
        let clamped = clamp_to_boundary(&p, p.items[1].start, p.items[1].radius);
        assert!(
            clamped.distance(center) <= 3.0 + 1e-4,
            "clamped to {clamped:?}, outside the r=3 circle"
        );
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
        let mut s = Scratch::new(2);
        let a = metrics(&wide, &Layout::from_starts(&wide.items), &mut s);
        let b = metrics(&tall, &Layout::from_starts(&tall.items), &mut s);
        assert!(
            (a.objective - b.objective).abs() < 1e-3,
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
        let m = metrics(&p, &Layout::from_starts(&p.items), &mut Scratch::new(2));
        assert_eq!(m.violations, 0, "nothing the solver could do about it");
    }
}
