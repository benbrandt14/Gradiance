//! The physics systems: collider/joint derivation, the plane seam, and the
//! read facade.
//!
//! **This crate is where two dimensions become three.** Authoring is
//! plane-local 2D everywhere else; [`plane`] holds the lift/project seam, and
//! the sync systems here are the only code that writes engine components.
//!
//! Ground planes need no special support: the ground tool authors an ordinary
//! static body with a half-plane shape.

pub mod body_sync;
pub mod clock;
pub mod fields;
pub mod forces;
pub mod grab;
pub mod hold;
pub mod joint_sync;
pub mod motor;
pub mod plane;
pub mod queries;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gradiance_core::states::GameState;
use gradiance_core::units::PlaneFrame;

/// Installs the physics engine, maps app state to the simulation clock, and
/// registers the authored→engine sync systems.
#[derive(Default)]
pub struct GradiancePhysicsPlugin;

impl Plugin for GradiancePhysicsPlugin {
    fn build(&self, app: &mut App) {
        // The world is SI: one world unit is one metre, so the engine's
        // length-based tolerances need no scaling (pixels live only at the
        // render/pick seam). Physics runs on the fixed schedule.
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule());
        app.init_resource::<gradiance_domain::settings::SimSettings>();
        // Reflected config seam for scripting (see script-lisp-decision.md).
        app.register_type::<gradiance_domain::settings::SimSettings>();
        app.add_systems(
            Update,
            apply_sim_settings.run_if(resource_changed::<gradiance_domain::settings::SimSettings>),
        );
        app.init_resource::<clock::SimClock>();
        app.add_systems(First, clock::sync_sim_clock);
        app.init_resource::<forces::ForceAccumulator>();
        app.init_resource::<hold::KinematicHold>();
        app.init_resource::<grab::MouseSpring>();
        app.init_resource::<grab::MouseTwist>();
        app.add_message::<fields::SetOrbitRequest>();
        app.register_type::<fields::SetOrbitRequest>();
        app.init_resource::<StepTrace>();
        app.add_systems(
            FixedPostUpdate,
            record_step_trace
                .after(PhysicsSet::Writeback)
                .run_if(step_trace_enabled),
        );
        app.add_systems(OnEnter(GameState::Paused), pause_physics);
        app.add_systems(OnExit(GameState::Paused), resume_physics);
        app.add_systems(
            Update,
            (
                hold::apply_kinematic_hold,
                grab::apply_mouse_spring,
                grab::apply_mouse_twist,
                fields::sync_field_mass,
                fields::apply_field_forces.run_if(in_state(GameState::Playing)),
                fields::apply_plane_friction.run_if(in_state(GameState::Playing)),
                fields::set_in_orbit,
            )
                .chain(),
        );
        // Every contributor accumulates first; one system hands the totals to
        // the engine and clears. Ordering is the whole contract, so it is
        // stated here rather than left to system-set inference.
        app.add_systems(
            Update,
            (forces::ensure_engine_components, forces::commit_forces)
                .chain()
                .after(grab::apply_mouse_twist)
                .after(fields::apply_field_forces)
                .after(fields::apply_plane_friction),
        );
        app.add_systems(
            PostUpdate,
            (
                forces::ensure_engine_components,
                body_sync::sync_body_physics,
                body_sync::sync_colliders,
                body_sync::sync_mass_properties,
                body_sync::sync_collision_groups,
                joint_sync::guard_dangling_joints,
                joint_sync::sync_joints,
            )
                .chain()
                .in_set(BodySyncSet),
        );
        app.add_systems(Update, motor::drive_oscillating_motors);
    }
}

/// System set for authored→engine body synchronization (runs in
/// `PostUpdate`, after commands mutated authored components in `Update`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodySyncSet;

/// Applies authored simulation settings to the engine.
fn apply_sim_settings(
    settings: Res<gradiance_domain::settings::SimSettings>,
    mut timestep: ResMut<TimestepMode>,
    mut config: Query<&mut RapierConfiguration>,
    mut fixed: ResMut<Time<Fixed>>,
) {
    let plane = PlaneFrame::XY;
    let hz = settings.timestep_hz.clamp(15.0, 240.0);
    let speed = settings.speed.clamp(0.0, 10.0);
    for mut cfg in &mut config {
        cfg.gravity = plane.dir(settings.gravity);
    }
    // Scaling the step rather than a clock ratio is what makes `speed = 0` a
    // true freeze: the solver is handed no time at all.
    *timestep = TimestepMode::Fixed {
        dt: speed / hz,
        substeps: settings.substeps.clamp(1, 64) as usize,
    };
    fixed.set_timestep_hz(f64::from(hz));
}

/// Positions of dynamic bodies at each of the last few physics **steps** — the
/// step debug view's data (feedback 8.3).
///
/// This used to be a per-*substep* trace. rapier runs its substeps inside a
/// single step call with no hook, and faking it by taking several
/// single-substep steps would be dishonest: independent steps re-run collision
/// detection and re-warm-start, so the picture would not show what the solver
/// actually did. A per-step trace is the honest version of the same overlay.
#[derive(Resource, Default, Debug)]
pub struct StepTrace(pub Vec<Vec<Vec2>>);

/// How many steps the trace holds.
const STEP_TRACE_DEPTH: usize = 8;

/// Records dynamic-body positions after each physics step, projected back to
/// plane-local coordinates at the recorder so no `Vec3` escapes this crate.
fn record_step_trace(
    mut trace: ResMut<StepTrace>,
    bodies: Query<(&Transform, &RigidBody), With<gradiance_domain::Body>>,
) {
    let plane = PlaneFrame::XY;
    if trace.0.len() >= STEP_TRACE_DEPTH {
        trace.0.remove(0);
    }
    trace.0.push(
        bodies
            .iter()
            .filter(|(_, kind)| **kind == RigidBody::Dynamic)
            .map(|(transform, _)| plane.project(transform.translation).0)
            .collect(),
    );
}

/// Run condition: the step debug view is enabled.
fn step_trace_enabled(debug: Res<gradiance_domain::settings::DebugSettings>) -> bool {
    debug.show_substeps
}

fn pause_physics(mut config: Query<&mut RapierConfiguration>) {
    for mut cfg in &mut config {
        cfg.physics_pipeline_active = false;
    }
}

fn resume_physics(mut config: Query<&mut RapierConfiguration>) {
    for mut cfg in &mut config {
        cfg.physics_pipeline_active = true;
    }
}
