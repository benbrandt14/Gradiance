//! The particle spike's **one ECS-touching seam**: authored emitters in,
//! derived particles out, on avian's fixed clock.
//!
//! This mirrors the field seam (`docs/field-architecture.md`) and honours
//! the invariants:
//!
//! - **Authored:** [`Emitter`] is a small reflect-registered component on a
//!   body (a spawn source). It is the only persisted/undoable part of the
//!   sim. *(Spike scope: emitters are spawned by the demo/`--features sim`
//!   startup and are not yet wired into the save file — that lands with the
//!   `gradiance-sim` crate, see `docs/mpm-trade-study.md`.)*
//! - **Derived:** [`Particles`] (the `SoA` buffer) is rebuilt every step —
//!   never serialized, never in undo, never read by commands.
//! - **No new mutation path.** Forces come through the *one* field
//!   cut-point [`Fields::accel_at`]; the population is bulk state the
//!   renderer reads, not authored components.
//! - **Tier-B only.** The per-frame work is the allocation-lean
//!   [`kernel`](crate::sim::kernel), never the scripting VM.

use crate::domain::settings::SimSettings;
use crate::physics::fields::Fields;
use crate::sim::kernel::{ParticleState, nbody_accelerations};
use bevy::prelude::*;

/// An authored-shaped particle source: a fountain/cloud spawner. Carried
/// on its own entity for the spike; it would later ride on an authored
/// body.
///
/// `Reflect` now (the registry/inspector bind to it); it gains the serde
/// derives — and moves into `src/domain/` — when it is wired into the save
/// file at the crate-split boundary (the serialization invariant confines
/// serde to authored data; `tests/boundaries.rs`).
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Emitter {
    /// Particles emitted per second.
    pub rate: f32,
    /// Launch speed (px/s) along the body's local +Y, before spread.
    pub speed: f32,
    /// Half-angle of the random launch spread (radians).
    pub spread: f32,
    /// Mass given to each particle (also its N-body mass).
    pub mass: f32,
    /// Seconds a particle lives before it is culled.
    pub max_age: f32,
    /// Self-gravity strength between particles (0 = a plain fountain;
    /// >0 makes the cloud clump — the `particular` N-body driver).
    pub self_gravity: f32,
    /// Group index its particles inherit (the locked-shared-attributes
    /// handle; one group per emitter for now — an authored `ParticleGroup`
    /// lands in S3). Drives render tint and future selection/aggregates.
    pub group: u32,
}

impl Default for Emitter {
    fn default() -> Self {
        Self {
            rate: 600.0,
            speed: 900.0,
            spread: 0.35,
            mass: 1.0,
            max_age: 3.0,
            self_gravity: 0.0,
            group: 0,
        }
    }
}

/// Tuning that is editor state, not authored: the global particle budget
/// and integration knobs. (Config-seam-shaped; not persisted in the spike.)
#[derive(Resource, Debug, Clone, Copy)]
pub struct SimConfig {
    /// Hard cap on live particles (oldest are culled first via `max_age`;
    /// this bounds memory when emitters over-produce).
    pub max_particles: usize,
    /// Per-second linear drag applied to every particle (air resistance).
    pub damping: f32,
    /// Softening length for the N-body solve (px), avoiding singularities.
    pub softening: f32,
    /// Whether scene gravity (`SimSettings::gravity`) acts on particles.
    pub feel_gravity: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            max_particles: 20_000,
            damping: 0.1,
            softening: 8.0,
            feel_gravity: true,
        }
    }
}

/// The derived particle population (`SoA`). Never serialized, never undoable.
#[derive(Resource, Debug, Default)]
pub struct Particles(pub ParticleState);

/// Fractional-particle carry so a fixed rate emits smoothly across steps
/// (e.g. 600/s at 60 Hz = exactly 10/step, but 601/s still averages right).
#[derive(Component, Debug, Default)]
pub struct EmitAccum(pub f32);

/// Spawns particles from each [`Emitter`] on the fixed clock, honouring the
/// global budget. Launch direction is the body's local +Y rotated by the
/// spread; position is the body origin.
pub fn emit_particles(
    time: Res<Time>,
    config: Res<SimConfig>,
    mut particles: ResMut<Particles>,
    mut emitters: Query<(&Emitter, &Transform, &mut EmitAccum)>,
    mut rng: Local<u32>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    // A tiny xorshift keeps the spike self-contained (no rng resource
    // plumbing); determinism is not required for a visual fountain.
    let next = |rng: &mut u32| {
        *rng ^= *rng << 13;
        *rng ^= *rng >> 17;
        *rng ^= *rng << 5;
        (*rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    if *rng == 0 {
        *rng = 0x9E37_79B9;
    }
    for (emitter, transform, mut accum) in &mut emitters {
        accum.0 += emitter.rate.max(0.0) * dt;
        let count = accum.0 as usize;
        accum.0 -= count as f32;
        let origin = transform.translation.truncate();
        let base = transform.rotation.to_euler(EulerRot::ZYX).0 + std::f32::consts::FRAC_PI_2;
        for _ in 0..count {
            if particles.0.len() >= config.max_particles {
                break;
            }
            let angle = base + next(&mut rng) * emitter.spread;
            let vel = Vec2::from_angle(angle) * emitter.speed;
            particles.0.push(origin, vel, emitter.mass, emitter.group);
        }
    }
}

/// Advances the population one fixed step: gathers external acceleration
/// (scene gravity + the field cut-point) and inter-particle N-body force,
/// integrates, then culls by age. One system, one buffer, no per-particle
/// entity — the Tier-B hot path.
pub fn step_particles(
    time: Res<Time>,
    config: Res<SimConfig>,
    sim: Res<SimSettings>,
    fields: Fields,
    mut particles: ResMut<Particles>,
    emitters: Query<&Emitter>,
) {
    let dt = time.delta_secs();
    let state = &mut particles.0;
    if dt <= 0.0 || state.is_empty() {
        return;
    }
    // Self-gravity strength: the max over emitters (spike-simple — one
    // global cloud). N-body acceleration is inter-particle only.
    let g = emitters
        .iter()
        .map(|e| e.self_gravity)
        .fold(0.0_f32, f32::max);
    let mut accel = nbody_accelerations(state, g, config.softening);
    if accel.is_empty() {
        accel = vec![Vec2::ZERO; state.len()];
    }
    let gravity = if config.feel_gravity {
        sim.gravity
    } else {
        Vec2::ZERO
    };
    // External acceleration per particle: scene gravity + the superposed
    // field (read through the single sanctioned cut-point).
    for (i, a) in accel.iter_mut().enumerate() {
        *a += gravity + fields.accel_at(state.pos[i], None);
    }
    state.integrate(dt, &accel, config.damping);
    // Cull aged particles (the budget is enforced at emit time).
    let max_age = emitters
        .iter()
        .map(|e| e.max_age)
        .fold(0.0_f32, f32::max)
        .max(0.5);
    state.cull_older_than(max_age);
}
