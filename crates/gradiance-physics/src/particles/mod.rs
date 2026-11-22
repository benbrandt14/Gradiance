//! Particle water (brief S-10, a spike): a derived particle population,
//! stepped by one system, that pushes bodies and is pushed by them.
//! The decision note is `docs/particle-fluid-decision.md`.

//!
//! - The population is **derived**: [`ParticleFluid`] holds it, nothing
//!   saves it, no command or undo record names it (CLAUDE.md two-tier
//!   rule: bulk updates are derived). In this spike only code fills it
//!   (tests, the spike scene); S-11 adds the authored emitter.
//! - One system, [`step_particle_fluid`], runs per physics step before
//!   avian's (in `FixedUpdate`, with the other force seams), in
//!   [`ParticleFluid::substeps`] substeps, against the bodies as the step
//!   starts. It pays each dynamic body the impulse its particles gave it,
//!   through avian's `Forces`: one-shot, never authored.
//! - The runtime gate: with no particles the system returns at once, so an
//!   app without water pays one resource read per step.

pub mod solver;

pub use solver::{Exchange, FluidParams, Obstacle, Particles, WallTable};

use crate::lifetime::Removed;
use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::fluid::Fluid;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::sdf;
use std::time::Duration;

/// The particle water of the scene and the solver's state.
#[derive(Resource, Debug, Clone)]
pub struct ParticleFluid {
    /// The particles.
    pub particles: Particles,
    /// The fluid's constants.
    pub params: FluidParams,
    /// Solver substeps per physics step.
    pub substeps: u32,
    /// The wall time the last step took (the spike's budget measure).
    pub last_step: Duration,
    rest_density: f32,
    walls: WallTable,
    exchange: Vec<Exchange>,
    total: Vec<Exchange>,
}

/// The default particle spacing, in metres.
pub const DEFAULT_SPACING: f32 = 0.04;

impl Default for ParticleFluid {
    fn default() -> Self {
        Self::water(DEFAULT_SPACING)
    }
}

impl ParticleFluid {
    /// Empty water at `spacing`.
    pub fn water(spacing: f32) -> Self {
        let params = FluidParams::water(spacing);
        Self {
            particles: Particles::default(),
            rest_density: params.rest_density(),
            walls: WallTable::new(&params),
            params,
            substeps: 4,
            last_step: Duration::ZERO,
            exchange: Vec::new(),
            total: Vec::new(),
        }
    }

    /// The areal mass density of water at rest, in kg/m² (the world is
    /// 1 m deep): one particle's mass per lattice cell.
    pub fn density(&self) -> f32 {
        self.params.particle_mass / (self.params.spacing * self.params.spacing)
    }
}

/// Installs the water resource and its step.
pub(crate) fn install(app: &mut App) {
    app.init_resource::<ParticleFluid>();
    app.add_systems(
        FixedUpdate,
        step_particle_fluid
            .after(crate::buoyancy::apply_buoyancy)
            .run_if(crate::sim_running),
    );
}

type ObstacleQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ShapeDef,
        &'static Position,
        &'static Rotation,
        &'static LinearVelocity,
        &'static AngularVelocity,
        &'static RigidBody,
        &'static ComputedCenterOfMass,
    ),
    (
        With<Body>,
        With<Collider>,
        Without<Removed>,
        Without<Fluid>,
        Without<Sensor>,
    ),
>;

/// Steps the water one physics step and pays the bodies their impulses.
#[allow(clippy::needless_pass_by_value)]
pub fn step_particle_fluid(
    mut fluid: ResMut<ParticleFluid>,
    mut bodies: ParamSet<(ObstacleQuery, Query<Forces, With<Body>>)>,
    gravity: Res<Gravity>,
    time: Res<Time<Fixed>>,
    mut owed: Local<Vec<(Entity, Vec2, f32)>>,
) {
    if fluid.particles.is_empty() {
        return;
    }
    let started = std::time::Instant::now();
    let substeps = fluid.substeps.max(1);
    let dt = time.timestep().as_secs_f32() / substeps as f32;
    owed.clear();
    {
        let query = bodies.p0();
        let mut entities = Vec::new();
        let mut obstacles = Vec::new();
        for (entity, shape, pos, rot, v, w, kind, com) in &query {
            let (min, max) = sdf::world_aabb(shape, pos.0, rot.as_radians());
            obstacles.push(Obstacle {
                shape,
                pos: pos.0,
                rot: rot.as_radians(),
                velocity: v.0,
                spin: w.0,
                min,
                max,
            });
            entities.push((entity, kind.is_dynamic(), *rot * com.0));
        }
        let ParticleFluid {
            particles,
            params,
            rest_density,
            walls,
            exchange,
            total,
            ..
        } = &mut *fluid;
        total.clear();
        total.resize(obstacles.len(), Exchange::default());
        for _ in 0..substeps {
            particles.step(
                params,
                *rest_density,
                walls,
                gravity.0,
                dt,
                &obstacles,
                exchange,
            );
            for (sum, e) in total.iter_mut().zip(exchange.iter()) {
                sum.impulse += e.impulse;
                sum.angular += e.angular;
            }
        }
        for ((entity, dynamic, com_offset), e) in entities.iter().zip(total.iter()) {
            if *dynamic && (e.impulse != Vec2::ZERO || e.angular != 0.0) {
                // The angular impulse was taken about the body's origin;
                // avian applies a linear impulse at the centre of mass.
                let about_com = e.angular - com_offset.perp_dot(e.impulse);
                owed.push((*entity, e.impulse, about_com));
            }
        }
    }
    let mut forces = bodies.p1();
    for (entity, impulse, angular) in owed.drain(..) {
        if let Ok(mut item) = forces.get_mut(entity) {
            item.apply_linear_impulse(impulse);
            item.apply_angular_impulse(angular);
        }
    }
    fluid.last_step = started.elapsed();
}
