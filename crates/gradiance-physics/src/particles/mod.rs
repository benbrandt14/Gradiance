//! Particle water (brief S-10, a spike): a derived particle population,
//! stepped by one system, that pushes bodies and is pushed by them.
//! The decision note is `docs/particle-fluid-decision.md`.

//!
//! - The population is **derived**: [`ParticleFluid`] holds it, nothing
//!   saves it, no command or undo record names it (CLAUDE.md two-tier
//!   rule: bulk updates are derived). The authored half is the water
//!   emitter (`domain::water`, a behaviour node, brief S-11): when the sim
//!   plays, [`fill_from_emitters`] fills each emitter once; a deleted
//!   emitter takes its particles with it, and undoing a run
//!   (`ClearRemovals`) empties the water so the next run refills it.
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
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::fluid::Fluid;
use gradiance_domain::lifetime::ClearRemovals;
use gradiance_domain::node::NodeKind;
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
    /// The emitter each particle came from (parallel to the particles).
    owners: Vec<StableId>,
    /// The emitters whose particles are out.
    filled: HashSet<StableId>,
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
            owners: Vec::new(),
            filled: HashSet::new(),
            rest_density: params.rest_density(),
            walls: WallTable::new(&params),
            params,
            substeps: 4,
            last_step: Duration::ZERO,
            exchange: Vec::new(),
            total: Vec::new(),
        }
    }

    /// Adds a particle at rest at `pos`, from the emitter `owner`.
    pub fn push(&mut self, pos: Vec2, owner: StableId) {
        self.particles.push(pos);
        self.owners.push(owner);
    }

    /// Empties the water (every emitter refills on the next run).
    pub fn clear(&mut self) {
        self.particles.clear();
        self.owners.clear();
        self.filled.clear();
    }

    /// The particles of the emitter `owner`.
    pub fn count_of(&self, owner: StableId) -> usize {
        self.owners.iter().filter(|o| **o == owner).count()
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
    app.add_systems(Update, drop_stale_water);
    app.add_systems(
        FixedUpdate,
        (fill_from_emitters, step_particle_fluid)
            .chain()
            .after(crate::buoyancy::apply_buoyancy)
            .run_if(crate::sim_running),
    );
}

/// Takes out the particles of every emitter that is gone (deleted, drained,
/// undone), and all of them when a run is undone.
pub fn drop_stale_water(
    mut fluid: ResMut<ParticleFluid>,
    mut clears: MessageReader<ClearRemovals>,
    emitters: Query<(&StableId, &NodeKind)>,
) {
    if clears.read().count() > 0 {
        if !fluid.particles.is_empty() || !fluid.filled.is_empty() {
            fluid.clear();
        }
        return;
    }
    if fluid.filled.is_empty() {
        return;
    }
    let live: HashSet<StableId> = emitters
        .iter()
        .filter(|(_, kind)| matches!(kind, NodeKind::Water(_)))
        .map(|(id, _)| *id)
        .collect();
    if fluid.filled.iter().all(|id| live.contains(id)) {
        return;
    }
    let ParticleFluid {
        particles,
        owners,
        filled,
        ..
    } = &mut *fluid;
    filled.retain(|id| live.contains(id));
    // Particles a caller pushed straight into `particles` have no owner
    // (the S-10 spike scene); only owned ones can go stale.
    if owners.len() == particles.len() {
        particles.retain(|i| live.contains(&owners[i]));
        owners.retain(|id| live.contains(id));
    }
}

/// Fills every water emitter not yet out, once, as the run plays.
pub fn fill_from_emitters(
    mut fluid: ResMut<ParticleFluid>,
    emitters: Query<(&StableId, &NodeKind, &Transform)>,
) {
    for (id, kind, transform) in &emitters {
        let NodeKind::Water(water) = kind else {
            continue;
        };
        if fluid.filled.contains(id) {
            continue;
        }
        // One water per scene: the first emitter out sets the spacing.
        if fluid.filled.is_empty() && fluid.particles.is_empty() {
            *fluid = ParticleFluid::water(water.spacing());
        }
        let at = transform.translation.truncate();
        let spacing = fluid.params.spacing;
        for p in gradiance_geometry::lattice::fill_polygon(&water.outline, spacing) {
            fluid.push(at + p, *id);
        }
        fluid.filled.insert(*id);
    }
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
