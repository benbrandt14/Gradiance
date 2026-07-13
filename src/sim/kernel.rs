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

    /// Appends one particle (no-op columns stay in lockstep).
    pub fn push(&mut self, pos: Vec2, vel: Vec2, mass: f32) {
        self.pos.push(pos);
        self.vel.push(vel);
        self.mass.push(mass.max(0.0));
        self.age.push(0.0);
    }

    /// Removes every particle older than `max_age` (swap-remove, so order
    /// is not preserved — particles are anonymous).
    pub fn cull_older_than(&mut self, max_age: f32) {
        let mut i = 0;
        while i < self.pos.len() {
            if self.age[i] > max_age {
                self.pos.swap_remove(i);
                self.vel.swap_remove(i);
                self.mass.swap_remove(i);
                self.age.swap_remove(i);
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

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting exact integrator arithmetic
mod tests {
    use super::*;

    #[test]
    fn integrate_is_symplectic_euler_with_drag() {
        let mut s = ParticleState::default();
        s.push(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0);
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
        s.push(Vec2::ZERO, Vec2::new(100.0, 0.0), 1.0);
        s.integrate(0.1, &[Vec2::ZERO], 5.0); // drag factor 1 - 0.5 = 0.5
        assert!((s.vel[0].x - 50.0).abs() < 1e-3);
    }

    #[test]
    fn culling_drops_only_the_aged() {
        let mut s = ParticleState::default();
        for _ in 0..5 {
            s.push(Vec2::ZERO, Vec2::ZERO, 1.0);
        }
        s.age = vec![0.1, 9.0, 0.2, 8.0, 0.3];
        s.cull_older_than(1.0);
        assert_eq!(s.len(), 3, "the two aged particles are gone");
        assert!(s.age.iter().all(|&a| a <= 1.0));
        // Every column stayed in lockstep.
        assert_eq!(s.pos.len(), s.vel.len());
        assert_eq!(s.vel.len(), s.mass.len());
        assert_eq!(s.mass.len(), s.age.len());
    }

    #[test]
    fn nbody_pulls_two_masses_together() {
        let mut s = ParticleState::default();
        s.push(Vec2::new(-10.0, 0.0), Vec2::ZERO, 1.0);
        s.push(Vec2::new(10.0, 0.0), Vec2::ZERO, 1.0);
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
        s.push(Vec2::ZERO, Vec2::ZERO, 1.0);
        s.push(Vec2::ONE, Vec2::ZERO, 1.0);
        assert_eq!(nbody_accelerations(&s, 0.0, 1.0), vec![Vec2::ZERO; 2]);
    }
}
