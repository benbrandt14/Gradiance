//! The particle **Tier-B kernel**: pure, allocation-lean numeric core for
//! the bulk-matter spike.
//!
//! Like `geometry/` and `script/kernel`, this module is pure — no ECS, no
//! `bevy_ecs` — so the integrator and the N-body force computation are
//! unit-testable in isolation and stay portable to the future
//! `gradiance-sim` crate (see `docs/mpm-trade-study.md`). The one external
//! dependency is `particular` (an N-body *accelerations* library, itself
//! pure numeric), which supplies the spike's inter-particle force; the
//! integrator and lifecycle are ours.
//!
//! Data is **structure-of-arrays** (`ParticleState`), the layout MPM will
//! reuse: parallel `Vec`s scanned in tight loops, never one ECS entity per
//! particle. Everything here is *derived* state — rebuilt each step,
//! never serialized, never undoable (CLAUDE.md invariant 5).

use crate::domain::shape::ShapeDef;
use crate::geometry::sdf;
use bevy::math::Vec2;
use particular::prelude::*;

/// A structure-of-arrays population of point particles. Index `i` names the
/// same particle across every column.
#[derive(Debug, Default, Clone)]
pub struct ParticleState {
    /// World position (px).
    pub pos: Vec<Vec2>,
    /// Velocity (px/s).
    pub vel: Vec<Vec2>,
    /// Mass (arbitrary units; also the N-body gravitational mass).
    pub mass: Vec<f32>,
    /// Seconds lived — drives culling.
    pub age: Vec<f32>,
    /// Owning group index (into the live group table). Particles inherit
    /// their emitter's group; it locks shared attributes (material, tint,
    /// field response) and is the handle selection/plots aggregate over.
    pub group: Vec<u32>,
}

/// Summary statistics over one particle group — what the plotters and
/// scripts read to "show average position/velocity" without touching the
/// per-particle buffer. Derived, never persisted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupAggregate {
    /// Number of live particles in the group.
    pub count: usize,
    /// Mass-weighted centroid (average position).
    pub centroid: Vec2,
    /// Mass-weighted mean velocity.
    pub mean_velocity: Vec2,
    /// Total kinetic energy `Σ ½·m·|v|²`.
    pub kinetic_energy: f32,
}

impl ParticleState {
    /// Live particle count.
    pub fn len(&self) -> usize {
        self.pos.len()
    }

    /// Whether the population is empty.
    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    /// Appends one particle in `group` (every column stays in lockstep).
    pub fn push(&mut self, pos: Vec2, vel: Vec2, mass: f32, group: u32) {
        self.pos.push(pos);
        self.vel.push(vel);
        self.mass.push(mass.max(0.0));
        self.age.push(0.0);
        self.group.push(group);
    }

    /// Removes every particle older than `max_age` (swap-remove, so order
    /// is not preserved — particles are anonymous).
    pub fn cull_older_than(&mut self, max_age: f32) {
        self.cull(|_, age| age > max_age);
    }

    /// Removes every particle older than its group's lifetime.
    /// `group_max_age[g]` is group `g`'s limit (`INFINITY` = never); a
    /// particle whose group index is out of range is kept.
    pub fn cull_by_group_age(&mut self, group_max_age: &[f32]) {
        self.cull(|group, age| {
            group_max_age
                .get(group as usize)
                .is_some_and(|&limit| age > limit)
        });
    }

    /// Aggregate statistics over the particles in `group` (mass-weighted
    /// centroid & mean velocity, count, kinetic energy) — the read primitive
    /// the plotters and scripts summarize a group with ("show average
    /// position/velocity"). Pure; `None` when the group has no particles.
    ///
    /// Reads are total (the governance rule), so this is a plain scan; it is
    /// never a mutation path. One pass, allocation-free.
    pub fn aggregate(&self, group: u32) -> Option<GroupAggregate> {
        let mut count = 0usize;
        let mut mass = 0.0f32;
        let mut pos = Vec2::ZERO;
        let mut vel = Vec2::ZERO;
        let mut ke = 0.0f32;
        for i in 0..self.pos.len() {
            if self.group[i] != group {
                continue;
            }
            let m = self.mass[i];
            count += 1;
            mass += m;
            pos += self.pos[i] * m;
            vel += self.vel[i] * m;
            ke += 0.5 * m * self.vel[i].length_squared();
        }
        if count == 0 {
            return None;
        }
        // Mass-weighted means; fall back to a plain count if the group is
        // entirely massless (degenerate but possible for test material).
        let w = if mass > 0.0 { mass } else { count as f32 };
        Some(GroupAggregate {
            count,
            centroid: pos / w,
            mean_velocity: vel / w,
            kinetic_energy: ke,
        })
    }

    /// Translates a transient spatial subset by `delta` (the *kinematic* tier
    /// of the two-tier selection model: moving a subset never touches the
    /// group's shared attributes — those are edited at the group level). Only
    /// positions change; out-of-range indices are ignored. Pure.
    pub fn translate_indices(&mut self, indices: &[usize], delta: Vec2) {
        for &i in indices {
            if let Some(p) = self.pos.get_mut(i) {
                *p += delta;
            }
        }
    }

    /// Removes a transient subset (delete-subset). Compacts every column with
    /// the same keep-mask, so order among survivors is preserved and
    /// duplicate/out-of-range indices are harmless. Pure.
    pub fn remove_indices(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        let mut keep = vec![true; self.pos.len()];
        for &i in indices {
            if let Some(flag) = keep.get_mut(i) {
                *flag = false;
            }
        }
        retain_mask(&mut self.pos, &keep);
        retain_mask(&mut self.vel, &keep);
        retain_mask(&mut self.mass, &keep);
        retain_mask(&mut self.age, &keep);
        retain_mask(&mut self.group, &keep);
    }

    /// Swap-removes every particle for which `drop(group, age)` is true.
    fn cull(&mut self, drop: impl Fn(u32, f32) -> bool) {
        let mut i = 0;
        while i < self.pos.len() {
            if drop(self.group[i], self.age[i]) {
                self.pos.swap_remove(i);
                self.vel.swap_remove(i);
                self.mass.swap_remove(i);
                self.age.swap_remove(i);
                self.group.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Semi-implicit (symplectic) Euler step: `v += a·dt`, then `x += v·dt`,
    /// with a per-second linear drag `damping` and age bookkeeping. `accel`
    /// must be one entry per particle (external + inter-particle summed by
    /// the caller). Allocation-free over the population.
    pub fn integrate(&mut self, dt: f32, accel: &[Vec2], damping: f32) {
        debug_assert_eq!(accel.len(), self.pos.len());
        let drag = (1.0 - damping * dt).clamp(0.0, 1.0);
        for (((pos, vel), a), age) in self
            .pos
            .iter_mut()
            .zip(self.vel.iter_mut())
            .zip(accel)
            .zip(self.age.iter_mut())
        {
            *vel = (*vel + *a * dt) * drag;
            *pos += *vel * dt;
            *age += dt;
        }
    }
}

/// Inter-particle gravitational acceleration for every particle, via
/// `particular`'s brute-force N-body solve (softened to avoid the
/// coincident-particle singularity). `g` scales mass into a gravitational
/// parameter (>0 attracts, so a cloud clumps; 0 disables). One entry per
/// particle, in input order. Returns an empty vec for `g == 0` so the
/// caller pays nothing when self-gravity is off.
pub fn nbody_accelerations(state: &ParticleState, g: f32, softening: f32) -> Vec<Vec2> {
    if g == 0.0 || state.len() < 2 {
        return vec![Vec2::ZERO; state.len()];
    }
    // `particular` reads tuples of ([x, y], mu) as particles; the returned
    // accelerations preserve order. Glam-free at the boundary.
    let particles: Vec<([f32; 2], f32)> = state
        .pos
        .iter()
        .zip(&state.mass)
        .map(|(p, m)| ([p.x, p.y], m * g))
        .collect();
    let mut cm = sequential::BruteForceSoftenedScalar { softening };
    particles
        .iter()
        .accelerations(&mut cm)
        .map(|a: [f32; 2]| Vec2::new(a[0], a[1]))
        .collect()
}

/// Compacts `column` in place, keeping element `i` iff `keep[i]`. `keep`
/// must be as long as `column`. Order among survivors is preserved.
fn retain_mask<T>(column: &mut Vec<T>, keep: &[bool]) {
    let mut k = 0;
    column.retain(|_| {
        let ok = keep[k];
        k += 1;
        ok
    });
}

/// Deterministic per-cell hash → a `u32` of jitter bits (two 16-bit halves).
fn cell_hash(seed: u32, x: u32, y: u32) -> u32 {
    let mut h = seed ^ x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

/// Samples approximately `target` points uniformly (an even jittered
/// lattice) inside `shape`, in the shape's **local frame**. The MPM /
/// "fill a body with particles" primitive: a body *becomes* a group of
/// material points. Reuses the SDF (`sdf::eval < 0` = inside) so any shape
/// — box, circle, polygon, CSG tree — fills correctly; half-planes (a
/// ground, not a region) yield nothing.
///
/// Pure and deterministic in `seed`; the caller applies the body's pose.
/// Returns up to ~`target` points (fewer for shapes that fill little of
/// their bounding box). The lattice spacing is sized from a coarse
/// inside-fraction estimate so the count lands near `target`.
pub fn fill_shape(shape: &ShapeDef, target: usize, seed: u32) -> Vec<Vec2> {
    // Coarse inside-fraction scan resolution — sizes the lattice to the
    // *shape's* area (not the bounding box) so the count lands near target.
    const SCAN: usize = 24;
    if target == 0 || matches!(shape, ShapeDef::HalfPlane) {
        return Vec::new();
    }
    let (min, max) = sdf::aabb(shape);
    let size = max - min;
    if size.x <= 0.0 || size.y <= 0.0 || !size.is_finite() {
        return Vec::new();
    }
    let mut inside = 0usize;
    for iy in 0..SCAN {
        for ix in 0..SCAN {
            let p = min
                + Vec2::new(
                    (ix as f32 + 0.5) / SCAN as f32 * size.x,
                    (iy as f32 + 0.5) / SCAN as f32 * size.y,
                );
            if sdf::eval(shape, p) < 0.0 {
                inside += 1;
            }
        }
    }
    let fill = (inside as f32 / (SCAN * SCAN) as f32).max(1.0 / (SCAN * SCAN) as f32);
    let spacing = (fill * size.x * size.y / target as f32).sqrt().max(1e-3);
    let cols = (size.x / spacing).ceil() as usize + 1;
    let rows = (size.y / spacing).ceil() as usize + 1;
    let mut pts = Vec::with_capacity(target);
    for iy in 0..rows {
        for ix in 0..cols {
            let h = cell_hash(seed, ix as u32, iy as u32);
            let jx = (h & 0xffff) as f32 / 65535.0;
            let jy = ((h >> 16) & 0xffff) as f32 / 65535.0;
            let p = min + Vec2::new((ix as f32 + jx) * spacing, (iy as f32 + jy) * spacing);
            if p.x <= max.x && p.y <= max.y && sdf::eval(shape, p) < 0.0 {
                pts.push(p);
            }
        }
    }
    pts
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting exact integrator arithmetic
mod tests {
    use super::*;

    #[test]
    fn integrate_is_symplectic_euler_with_drag() {
        let mut s = ParticleState::default();
        s.push(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, 0);
        // No accel, no drag: position advances by v·dt, velocity unchanged.
        s.integrate(0.5, &[Vec2::ZERO], 0.0);
        assert_eq!(s.vel[0], Vec2::new(10.0, 0.0));
        assert_eq!(s.pos[0], Vec2::new(5.0, 0.0));
        assert_eq!(s.age[0], 0.5);
        // Constant accel bumps velocity first, then position uses the new v.
        s.integrate(1.0, &[Vec2::new(0.0, 2.0)], 0.0);
        assert_eq!(s.vel[0], Vec2::new(10.0, 2.0));
        assert_eq!(s.pos[0], Vec2::new(15.0, 2.0));
    }

    #[test]
    fn drag_bleeds_velocity() {
        let mut s = ParticleState::default();
        s.push(Vec2::ZERO, Vec2::new(100.0, 0.0), 1.0, 0);
        s.integrate(0.1, &[Vec2::ZERO], 5.0); // drag factor 1 - 0.5 = 0.5
        assert!((s.vel[0].x - 50.0).abs() < 1e-3);
    }

    #[test]
    fn culling_drops_only_the_aged() {
        let mut s = ParticleState::default();
        for _ in 0..5 {
            s.push(Vec2::ZERO, Vec2::ZERO, 1.0, 0);
        }
        s.age = vec![0.1, 9.0, 0.2, 8.0, 0.3];
        s.cull_older_than(1.0);
        assert_eq!(s.len(), 3, "the two aged particles are gone");
        assert!(s.age.iter().all(|&a| a <= 1.0));
        // Every column stayed in lockstep.
        assert_eq!(s.pos.len(), s.vel.len());
        assert_eq!(s.vel.len(), s.mass.len());
        assert_eq!(s.mass.len(), s.age.len());
        assert_eq!(s.age.len(), s.group.len());
    }

    #[test]
    fn cull_by_group_age_uses_each_group_lifetime() {
        let mut s = ParticleState::default();
        s.push(Vec2::ZERO, Vec2::ZERO, 1.0, 0); // group 0: short-lived
        s.push(Vec2::ZERO, Vec2::ZERO, 1.0, 1); // group 1: persists
        s.push(Vec2::ZERO, Vec2::ZERO, 1.0, 0);
        s.age = vec![5.0, 5.0, 0.1];
        // Group 0 lifetime 1.0, group 1 infinite.
        s.cull_by_group_age(&[1.0, f32::INFINITY]);
        assert_eq!(s.len(), 2, "the aged group-0 particle is culled");
        assert!(
            s.group.contains(&1),
            "the persistent group-1 particle stays"
        );
    }

    #[test]
    fn translate_and_remove_subset_keep_columns_in_lockstep() {
        let mut s = ParticleState::default();
        for i in 0..5 {
            s.push(Vec2::new(i as f32, 0.0), Vec2::ONE, 1.0, i as u32);
        }
        // Move indices 1 and 3 by (+10, +10) — positions only, attrs untouched.
        s.translate_indices(&[1, 3], Vec2::new(10.0, 10.0));
        assert_eq!(s.pos[1], Vec2::new(11.0, 10.0));
        assert_eq!(s.pos[3], Vec2::new(13.0, 10.0));
        assert_eq!(
            s.vel[1],
            Vec2::ONE,
            "velocity (a shared/physical attr) untouched"
        );
        assert_eq!(s.pos[0], Vec2::new(0.0, 0.0), "unselected particle stays");
        // Out-of-range index is a no-op.
        s.translate_indices(&[99], Vec2::new(1.0, 0.0));

        // Remove indices 0 and 2 (and a duplicate/out-of-range, harmlessly).
        s.remove_indices(&[2, 0, 2, 42]);
        assert_eq!(s.len(), 3);
        // Survivors keep their order: original indices 1, 3, 4 → groups 1,3,4.
        assert_eq!(s.group, vec![1, 3, 4]);
        // Every column compacted in lockstep.
        assert_eq!(s.pos.len(), 3);
        assert_eq!(s.vel.len(), 3);
        assert_eq!(s.mass.len(), 3);
        assert_eq!(s.age.len(), 3);
        // The moved particle (was index 1) survived with its translation.
        assert_eq!(s.pos[0], Vec2::new(11.0, 10.0));
    }

    #[test]
    fn aggregate_summarizes_a_group() {
        let mut s = ParticleState::default();
        // Group 0: two particles straddling the origin, moving oppositely.
        s.push(Vec2::new(-10.0, 0.0), Vec2::new(0.0, 4.0), 1.0, 0);
        s.push(Vec2::new(10.0, 0.0), Vec2::new(0.0, -4.0), 1.0, 0);
        // Group 1: one particle elsewhere.
        s.push(Vec2::new(100.0, 50.0), Vec2::new(2.0, 0.0), 2.0, 1);

        let a = s.aggregate(0).expect("group 0 populated");
        assert_eq!(a.count, 2);
        assert_eq!(a.centroid, Vec2::ZERO, "centroid at the origin");
        assert_eq!(a.mean_velocity, Vec2::ZERO, "opposite velocities cancel");
        // KE = ½·1·16 + ½·1·16 = 16.
        assert!((a.kinetic_energy - 16.0).abs() < 1e-4);

        let b = s.aggregate(1).expect("group 1 populated");
        assert_eq!(b.count, 1);
        assert_eq!(b.centroid, Vec2::new(100.0, 50.0));
        assert_eq!(b.mean_velocity, Vec2::new(2.0, 0.0));

        assert!(s.aggregate(7).is_none(), "empty group → None");
    }

    #[test]
    fn nbody_pulls_two_masses_together() {
        let mut s = ParticleState::default();
        s.push(Vec2::new(-10.0, 0.0), Vec2::ZERO, 1.0, 0);
        s.push(Vec2::new(10.0, 0.0), Vec2::ZERO, 1.0, 0);
        let acc = nbody_accelerations(&s, 1000.0, 1.0);
        assert_eq!(acc.len(), 2);
        // Left particle accelerates right (toward the other), and vice versa.
        assert!(acc[0].x > 0.0, "left pulled toward right: {:?}", acc[0]);
        assert!(acc[1].x < 0.0, "right pulled toward left: {:?}", acc[1]);
        assert!(acc[0].y.abs() < 1e-3 && acc[1].y.abs() < 1e-3);
    }

    #[test]
    fn nbody_is_free_when_disabled() {
        let mut s = ParticleState::default();
        s.push(Vec2::ZERO, Vec2::ZERO, 1.0, 0);
        s.push(Vec2::ONE, Vec2::ZERO, 1.0, 0);
        assert_eq!(nbody_accelerations(&s, 0.0, 1.0), vec![Vec2::ZERO; 2]);
    }

    #[test]
    fn fill_shape_fills_a_box_with_contained_points_near_target() {
        let shape = ShapeDef::Box {
            width: 100.0,
            height: 100.0,
        };
        let pts = fill_shape(&shape, 400, 7);
        // A box fills its whole AABB, so the count lands close to target.
        assert!(
            (200..=650).contains(&pts.len()),
            "≈400 points, got {}",
            pts.len()
        );
        assert!(
            pts.iter().all(|p| p.x.abs() <= 50.0 && p.y.abs() <= 50.0),
            "every point is inside the box"
        );
    }

    #[test]
    fn fill_shape_respects_a_circles_boundary() {
        let shape = ShapeDef::Circle { radius: 50.0 };
        let pts = fill_shape(&shape, 300, 3);
        assert!(!pts.is_empty());
        assert!(
            pts.iter().all(|p| p.length() <= 50.0 + 1e-3),
            "every point is within the radius"
        );
    }

    #[test]
    fn fill_shape_is_deterministic_and_guards_degenerates() {
        let shape = ShapeDef::Circle { radius: 20.0 };
        assert_eq!(fill_shape(&shape, 100, 42), fill_shape(&shape, 100, 42));
        assert!(!fill_shape(&shape, 100, 1).is_empty());
        assert!(!fill_shape(&shape, 100, 2).is_empty());
        assert!(fill_shape(&shape, 0, 0).is_empty(), "target 0 → empty");
        assert!(
            fill_shape(&ShapeDef::HalfPlane, 100, 0).is_empty(),
            "a half-plane is not a fillable region"
        );
    }
}
