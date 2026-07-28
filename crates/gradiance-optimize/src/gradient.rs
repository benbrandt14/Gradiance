//! The differentiable view of a packing problem: parameters in, cost and
//! gradient out.
//!
//! This is the adapter that lets a real optimization library drive the
//! packing. A layout becomes a flat parameter vector — `[x, y, θ]` per
//! **movable** item, pinned items excluded so the optimizer never wastes
//! coordinates on things it cannot change — and [`PackEnergy`] answers the
//! two questions any gradient method asks: what does this cost, and which
//! way is downhill.
//!
//! # The surrogate, and why it is not the real objective
//!
//! [`PackEnergy::cost`] does **not** return
//! [`Metrics::objective`](crate::Metrics::objective). This is the single most
//! important thing about this module, and it was learned the hard way: a
//! line search requires the value it is handed to be the function whose
//! gradient it is handed. Give it a cost from one function and a gradient
//! from another and it will hunt for a step that satisfies the Wolfe
//! conditions, never find one, and — in argmin's Hager–Zhang implementation,
//! whose bracketing update is an unbounded loop — spin forever rather than
//! return an error.
//!
//! So the gradient path optimizes a **surrogate**: a smooth relaxation built
//! only from terms that differentiate exactly.
//!
//! | surrogate term | stands in for |
//! |---|---|
//! | `Σ (clearance − d)²` over violating pairs | the overlap penalty |
//! | `Σ (d − clearance)²` over near pairs | the gap term |
//! | `Σ ‖pos − centre‖²` | the extent/fill terms |
//!
//! Squared rather than linear because the linear form has a kink exactly at
//! contact, which is where the solver spends all its time. The bounding box,
//! the convex hull, and the edge-alignment fold are all absent: a bounding
//! box is a `max` over vertices, so its derivative is supported on whichever
//! single vertex is currently extreme, and following it moves one body at a
//! time and stalls. `‖pos − centre‖²` is the smooth thing that actually
//! wants the same arrangement.
//!
//! The **run still judges by the real objective** —
//! [`PackRun`](crate::PackRun) scores every iterate with
//! [`metrics`](crate::metrics), so the best-so-far, the convergence test and
//! the UI readout are all on the true measure. The surrogate only steers the
//! search.
//!
//! # Where the gradient comes from
//!
//! Translation is exact and nearly free, because SAT already computes it.
//! For a violating or near pair, the separating axis `n` is exactly the
//! direction along which the pair's distance changes fastest:
//!
//! ```text
//! ∂(separation)/∂posᵢ = −n        ∂(separation)/∂posⱼ = +n
//! ```
//!
//! so both pair terms differentiate to a sum of `±n` contributions — the
//! same quantity SAT already yields as a minimum translation. The
//! compaction term differentiates to `2(pos − centre)`. No finite
//! differences, no extra evaluations.
//!
//! Rotation is the exception, and only under
//! [`RotationMode::Free`](crate::RotationMode::Free): every quantized mode
//! snaps in [`PackEnergy::to_layout`], so the surrogate is piecewise
//! constant in θ and a finite difference reads exactly zero almost
//! everywhere — those modes are left to the search solvers, which handle
//! discrete orientation properly. Free rotation gets one forward difference
//! per item, of the *same* surrogate, so it stays consistent.

use bevy::math::Vec2;
use gradiance_core::units::PosRot;

use crate::objective::{Scratch, clamp_to_boundary};
use crate::problem::{Layout, PackProblem};
use gradiance_geometry::sat::separation;

/// Degrees of freedom per movable item: x, y, and rotation.
pub const DOF: usize = 3;

/// How much more the surrogate cares about penetration than about compaction.
///
/// A quadratic penalty has a **vanishing gradient at contact**: `d/dd (d²)`
/// is zero at `d = 0`, so the first fraction of a millimetre of overlap costs
/// almost nothing while the compaction term keeps pulling with full force.
/// Left alone, descent settles a hair inside every neighbour — enough to be
/// infeasible, which the run then rejects wholesale, so the solver appears to
/// do nothing at all.
///
/// The fix is to make the penalty steep enough that the equilibrium
/// penetration lands below `Metrics::PENETRATION_TOLERANCE`. Balancing the
/// two gradients gives a required ratio on the order of
/// `arrangement_size / tolerance`, hence four orders of magnitude. Steep
/// penalties make the problem stiff, which is exactly what a quasi-Newton
/// method with curvature history is good at.
const OVERLAP_DOMINANCE: f64 = 1e4;

/// A packing problem presented as a differentiable energy over a flat
/// parameter vector.
pub struct PackEnergy<'a> {
    problem: &'a PackProblem,
    movable: Vec<usize>,
    /// The compaction target, cached: it depends only on the *start* poses,
    /// so recomputing it per evaluation would be waste — and letting it move
    /// with the layout would make the energy non-stationary.
    center: Vec2,
}

/// The surrogate's three folded weights.
#[derive(Debug, Clone, Copy)]
struct SurrogateWeights {
    overlap: f64,
    gap: f64,
    compaction: f64,
}

/// The gap term is a mean over near pairs, so each pair's share shrinks as
/// more of them come into range.
fn gap_scale(weight: f64, count: u32) -> f64 {
    if count == 0 {
        0.0
    } else {
        weight / f64::from(count)
    }
}

impl<'a> PackEnergy<'a> {
    /// Wraps `problem`, fixing the parameter ordering for this run.
    pub fn new(problem: &'a PackProblem) -> Self {
        Self {
            movable: problem.movable_indices(),
            center: problem.start_center(),
            problem,
        }
    }

    /// The problem being differentiated.
    pub fn problem(&self) -> &PackProblem {
        self.problem
    }

    /// Number of parameters (`DOF` per movable item).
    pub fn dim(&self) -> usize {
        self.movable.len() * DOF
    }

    /// Flattens a layout into the parameter vector.
    pub fn to_params(&self, layout: &Layout) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.dim());
        for &i in &self.movable {
            let fallback = self.problem.items.get(i).map_or(
                PosRot {
                    pos: Vec2::ZERO,
                    rot: 0.0,
                },
                |item| item.start,
            );
            let pose = layout.poses.get(i).copied().unwrap_or(fallback);
            out.push(f64::from(pose.pos.x));
            out.push(f64::from(pose.pos.y));
            out.push(f64::from(pose.rot));
        }
        out
    }

    /// Rebuilds a layout from the parameter vector, leaving pinned items at
    /// their start poses and applying the rotation mode and any hard
    /// boundary — so a parameter vector can never describe an arrangement
    /// the rest of the crate would consider malformed.
    pub fn to_layout(&self, params: &[f64]) -> Layout {
        let mut layout = Layout::from_starts(&self.problem.items);
        for (slot, &i) in self.movable.iter().enumerate() {
            let base = slot * DOF;
            let (Some(&x), Some(&y), Some(&rot)) =
                (params.get(base), params.get(base + 1), params.get(base + 2))
            else {
                continue;
            };
            let Some(item) = self.problem.items.get(i) else {
                continue;
            };
            let mut pose = PosRot {
                pos: Vec2::new(x as f32, y as f32),
                rot: self
                    .problem
                    .config
                    .rotation
                    .snap(rot as f32, item.start.rot),
            };
            if !pose.pos.is_finite() || !pose.rot.is_finite() {
                pose = item.start;
            }
            pose.pos = clamp_to_boundary(self.problem, pose, item.radius);
            layout.poses[i] = pose;
        }
        layout
    }

    /// The **surrogate** energy at `params`.
    ///
    /// Deliberately not [`Metrics::objective`](crate::Metrics::objective).
    /// See the module docs: a gradient method's line search requires the
    /// value it is given to be the function whose gradient it is given, and
    /// the full objective contains terms (a bounding box, a convex hull, an
    /// edge-alignment fold) that are not differentiable in any useful way.
    pub fn cost(&self, params: &[f64], scratch: &mut Scratch) -> f64 {
        let layout = self.to_layout(params);
        scratch.refresh(self.problem, &layout);
        self.surrogate(scratch)
    }

    /// The surrogate energy of whatever is currently placed in `scratch`.
    ///
    /// Three squared terms, each exactly differentiable:
    ///
    /// - **overlap** — `Σ (clearance − d)²` over violating pairs.
    /// - **gap** — `Σ (d − clearance)²` over near pairs, the local density
    ///   term.
    /// - **compaction** — `Σ ‖pos − centre‖²`, a smooth stand-in for the
    ///   extent measure, weighted by the user's `extent` + `fill` dials.
    ///
    /// Squared rather than linear because a line search wants a `C¹`
    /// function; the linear form has a kink exactly at contact, which is
    /// where the solver spends all of its time.
    pub fn surrogate(&self, scratch: &Scratch) -> f64 {
        let (overlap, gap, count) = self.pair_energies(scratch);
        let w = self.weights();
        let compaction: f64 = self
            .movable
            .iter()
            .map(|&i| f64::from(scratch.center(i).distance_squared(self.center)))
            .sum();
        w.overlap * overlap
            + gap_scale(w.gap, count) * gap
            + w.compaction * compaction / self.movable.len().max(1) as f64
    }

    /// Squared overlap, squared gap, and the near-pair count.
    fn pair_energies(&self, scratch: &Scratch) -> (f64, f64, u32) {
        let clearance = self.problem.config.clearance;
        let (mut overlap, mut gap, mut count) = (0.0, 0.0, 0u32);
        self.for_each_near_pair(scratch, |_, _, sep| {
            count += 1;
            let delta = f64::from(sep.distance - clearance);
            if delta < 0.0 {
                overlap += delta * delta;
            } else {
                gap += delta * delta;
            }
        });
        (overlap, gap, count)
    }

    /// Cost and gradient at `params`, both of the surrogate.
    ///
    /// Returns both because every caller needs both and the cost falls out
    /// of the gradient computation for free.
    pub fn cost_and_gradient(&self, params: &[f64], scratch: &mut Scratch) -> (f64, Vec<f64>) {
        let layout = self.to_layout(params);
        scratch.refresh(self.problem, &layout);
        let base = self.surrogate(scratch);

        let mut grad = vec![0.0_f64; self.dim()];
        self.accumulate_pair_gradient(scratch, &mut grad);
        self.accumulate_compaction_gradient(scratch, &mut grad);
        // Only free rotation has a slope worth probing: every quantized mode
        // snaps in `to_layout`, so the surrogate is piecewise constant in θ
        // and a finite difference reads exactly zero almost everywhere. The
        // quantized modes are handled by the search solvers instead.
        if matches!(self.problem.config.rotation, crate::RotationMode::Free) {
            self.accumulate_rotation_gradient(params, base, scratch, &mut grad);
        }
        (base, grad)
    }

    /// The surrogate's weights, folded once.
    fn weights(&self) -> SurrogateWeights {
        let cfg = &self.problem.config;
        let length_ref = self.problem.total_area().max(1e-9).sqrt();
        let scale = f64::from(1.0 / (length_ref * length_ref));
        SurrogateWeights {
            overlap: f64::from(cfg.overlap_penalty.max(1.0)) * OVERLAP_DOMINANCE * scale,
            gap: f64::from(cfg.weights.gap) * scale,
            compaction: f64::from(cfg.weights.extent + cfg.weights.fill) * scale,
        }
    }

    /// Exact translation gradient of the overlap and gap terms.
    fn accumulate_pair_gradient(&self, scratch: &Scratch, grad: &mut [f64]) {
        let clearance = self.problem.config.clearance;
        let w = self.weights();
        let (_, _, count) = self.pair_energies(scratch);
        let gap_w = gap_scale(w.gap, count);

        let slot_of = |i: usize| self.movable.iter().position(|m| *m == i);
        self.for_each_near_pair(scratch, |i, j, sep| {
            let delta = f64::from(sep.distance - clearance);
            // d(term)/d(distance): the squared forms differentiate to 2·delta
            // scaled by their weight, with the sign falling out of `delta`.
            let dcost_ddist = if delta < 0.0 {
                2.0 * w.overlap * delta
            } else {
                2.0 * gap_w * delta
            };
            if dcost_ddist == 0.0 {
                return;
            }
            // distance grows as j moves along +axis and i along −axis.
            let axis = sep.axis;
            if let Some(slot) = slot_of(j) {
                grad[slot * DOF] += dcost_ddist * f64::from(axis.x);
                grad[slot * DOF + 1] += dcost_ddist * f64::from(axis.y);
            }
            if let Some(slot) = slot_of(i) {
                grad[slot * DOF] -= dcost_ddist * f64::from(axis.x);
                grad[slot * DOF + 1] -= dcost_ddist * f64::from(axis.y);
            }
        });
    }

    /// Exact gradient of the compaction term.
    fn accumulate_compaction_gradient(&self, scratch: &Scratch, grad: &mut [f64]) {
        let w = self.weights();
        if w.compaction == 0.0 || self.movable.is_empty() {
            return;
        }
        let scale = 2.0 * w.compaction / self.movable.len() as f64;
        for (slot, &i) in self.movable.iter().enumerate() {
            let d = scratch.center(i) - self.center;
            grad[slot * DOF] += scale * f64::from(d.x);
            grad[slot * DOF + 1] += scale * f64::from(d.y);
        }
    }

    /// One forward difference per rotating item, on the same surrogate.
    fn accumulate_rotation_gradient(
        &self,
        params: &[f64],
        base_cost: f64,
        scratch: &mut Scratch,
        grad: &mut [f64],
    ) {
        const H: f64 = 1e-3;
        let mut probe = params.to_vec();
        for slot in 0..self.movable.len() {
            let index = slot * DOF + 2;
            let Some(original) = probe.get(index).copied() else {
                continue;
            };
            probe[index] = original + H;
            let cost = self.cost(&probe, scratch);
            probe[index] = original;
            if cost.is_finite() {
                grad[index] += (cost - base_cost) / H;
            }
        }
        // The probe left the scratch holding a perturbed layout; restore it
        // so callers can keep reading placements.
        let layout = self.to_layout(params);
        scratch.refresh(self.problem, &layout);
    }

    /// Visits every pair the surrogate cares about, with its separation.
    ///
    /// Two sources, unioned: pairs close enough to be overlapping (the
    /// broad-phase scan) and each item's `gap_neighbors` nearest neighbours
    /// (the gap term's fixed-count set). The gap set has to be a count
    /// rather than a radius — with a radius, spreading everything apart
    /// empties the set and the energy reads zero, so "explode the
    /// arrangement" becomes a global minimum.
    fn for_each_near_pair(
        &self,
        scratch: &Scratch,
        mut f: impl FnMut(usize, usize, gradiance_geometry::sat::Separation),
    ) {
        let problem = self.problem;
        let cfg = &problem.config;
        let n = problem.items.len();
        let centers: Vec<bevy::math::Vec2> = (0..n).map(|i| scratch.center(i)).collect();
        let mut pairs = problem.nearest_pairs(&centers, cfg.gap_neighbors as usize);

        for i in 0..n {
            for j in (i + 1)..n {
                if !problem.pair_collides(i, j) {
                    continue;
                }
                if !problem.movable(i) && !problem.movable(j) {
                    continue;
                }
                let (Some(a), Some(b)) = (problem.items.get(i), problem.items.get(j)) else {
                    continue;
                };
                let broad = a.radius + b.radius + cfg.clearance;
                if scratch.center(i).distance_squared(scratch.center(j)) <= broad * broad
                    && !pairs.contains(&(i, j))
                {
                    pairs.push((i, j));
                }
            }
        }

        for (i, j) in pairs {
            if let Some(sep) = separation(scratch.placed(i), scratch.placed(j)) {
                f(i, j, sep);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting an exactly-zero slope
mod tests {
    use super::*;
    use crate::problem::{PackConfig, PackItem, RotationMode};

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

    fn problem(items: Vec<PackItem>) -> PackProblem {
        PackProblem::new(
            items,
            PackConfig {
                clearance: 0.0,
                rotation: RotationMode::Fixed,
                ..Default::default()
            },
        )
    }

    #[test]
    fn parameters_round_trip_through_a_layout() {
        let p = problem(vec![
            square(Vec2::ZERO, 0.5),
            square(Vec2::new(3.0, 1.0), 0.5),
        ]);
        let energy = PackEnergy::new(&p);
        assert_eq!(energy.dim(), 6, "two movable items, three DOF each");
        let start = Layout::from_starts(&p.items);
        let params = energy.to_params(&start);
        let back = energy.to_layout(&params);
        for (a, b) in start.poses.iter().zip(&back.poses) {
            assert!(a.pos.distance(b.pos) < 1e-5);
        }
    }

    #[test]
    fn pinned_items_get_no_parameters_and_never_move() {
        let mut anchor = square(Vec2::ZERO, 0.5);
        anchor.pinned = true;
        let anchor_start = anchor.start;
        let p = problem(vec![anchor, square(Vec2::new(3.0, 0.0), 0.5)]);
        let energy = PackEnergy::new(&p);
        assert_eq!(energy.dim(), DOF, "only the free item is a parameter");
        // Whatever the parameters say, the pinned item stays put.
        let layout = energy.to_layout(&[99.0, 99.0, 0.0]);
        assert!(layout.poses[0].pos.distance(anchor_start.pos) < 1e-6);
    }

    #[test]
    fn the_gradient_points_downhill() {
        // Two squares with a gap: a step along the negative gradient must
        // reduce the cost. This is the property every gradient method relies
        // on, and the only one that matters for an approximate gradient.
        let p = problem(vec![
            square(Vec2::new(-1.5, 0.0), 0.5),
            square(Vec2::new(1.5, 0.0), 0.5),
        ]);
        let energy = PackEnergy::new(&p);
        let mut scratch = Scratch::new(p.len());
        let params = energy.to_params(&Layout::from_starts(&p.items));
        let (cost, grad) = energy.cost_and_gradient(&params, &mut scratch);
        assert!(grad.iter().any(|g| g.abs() > 1e-9), "a real gradient");

        let norm = grad.iter().map(|g| g * g).sum::<f64>().sqrt();
        let stepped: Vec<f64> = params
            .iter()
            .zip(&grad)
            .map(|(p, g)| p - 0.05 * g / norm)
            .collect();
        let after = energy.cost(&stepped, &mut scratch);
        assert!(
            after < cost,
            "stepping downhill must lower the cost: {cost} -> {after}"
        );
    }

    #[test]
    fn the_gradient_separates_an_overlapping_pair() {
        let p = problem(vec![
            square(Vec2::new(-0.2, 0.0), 0.5),
            square(Vec2::new(0.2, 0.0), 0.5),
        ]);
        let energy = PackEnergy::new(&p);
        let mut scratch = Scratch::new(p.len());
        let params = energy.to_params(&Layout::from_starts(&p.items));
        let (_, grad) = energy.cost_and_gradient(&params, &mut scratch);
        // Item 0 is on the left, so descending must move it further left
        // (its gradient x-component must be positive).
        assert!(grad[0] > 0.0, "left item pushed left, grad was {}", grad[0]);
        assert!(grad[DOF] < 0.0, "right item pushed right");
    }

    #[test]
    fn a_fixed_rotation_run_leaves_the_rotation_gradient_alone() {
        let p = problem(vec![
            square(Vec2::new(-1.0, 0.0), 0.5),
            square(Vec2::new(1.0, 0.0), 0.5),
        ]);
        let energy = PackEnergy::new(&p);
        let mut scratch = Scratch::new(p.len());
        let params = energy.to_params(&Layout::from_starts(&p.items));
        let (_, grad) = energy.cost_and_gradient(&params, &mut scratch);
        assert_eq!(grad[2], 0.0, "rotation is fixed, so it has no slope");
        assert_eq!(grad[DOF + 2], 0.0);
    }

    #[test]
    fn non_finite_parameters_fall_back_to_the_start_pose() {
        let p = problem(vec![
            square(Vec2::ZERO, 0.5),
            square(Vec2::new(3.0, 0.0), 0.5),
        ]);
        let energy = PackEnergy::new(&p);
        let layout = energy.to_layout(&[f64::NAN, 0.0, 0.0, 1.0, 1.0, 0.0]);
        assert!(layout.poses[0].pos.is_finite(), "NaN must not reach a pose");
    }
}
