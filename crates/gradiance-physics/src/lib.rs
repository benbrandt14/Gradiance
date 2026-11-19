//! The physics systems: collider/joint derivation and the read facade.
//!
//! Post de-adapter (`docs/physics-deadapter-decision.md`) authored physics
//! state *is* avian components on the entity; this module no longer translates
//! a mirror. It still owns the genuine derivations — `ShapeDef` → `Collider`
//! and `DepthBand` → `CollisionLayers` ([`body_sync`]) — and the
//! [`queries`] read facade. avian is used directly here and elsewhere.
//!
//! Ground planes need no special support here: the ground tool authors an
//! ordinary `Static` body with a large box shape.

pub mod body_sync;
pub mod fields;
pub mod grab;
pub mod hold;
pub mod joint_sync;
pub mod motor;
pub mod queries;

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_core::constants::GRAVITY;
use gradiance_core::states::GameState;

/// Installs avian, maps app state to the physics clock, and registers the
/// authored→engine sync systems.
#[derive(Default)]
pub struct GradiancePhysicsPlugin;

impl Plugin for GradiancePhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Islands (sleeping optimization) are disabled: avian 0.7's island
        // bookkeeping panics when a contact pair is removed by inserting
        // `JointCollisionDisabled` on already-touching bodies — a routine
        // editor operation (welding touching boxes). Avian documents the
        // plugin as safely omittable.
        app.add_plugins(
            PhysicsPlugins::default()
                // The world is SI: 1 world unit = 1 metre, so avian's
                // length-based tolerances need no scaling (pixels live only at
                // the render/pick seam).
                .with_length_unit(1.0)
                .build()
                .disable::<IslandPlugin>(),
        );
        // The collider picking backend needs bevy's core picking plugin
        // (present under DefaultPlugins, absent in headless test apps).
        if app.is_plugin_added::<bevy::picking::PickingPlugin>() {
            app.add_plugins(PhysicsPickingPlugin);
        }
        app.insert_resource(Gravity(GRAVITY));
        app.init_resource::<gradiance_domain::settings::SimSettings>();
        // Reflected config seam for scripting (see script-lisp-decision.md).
        app.register_type::<gradiance_domain::settings::SimSettings>();
        // `PostUpdate`, not `Update`: every writer of the settings (the
        // settings window, the context menu, a script's `sim-set`) runs in
        // `Update`, and the next physics step runs at the start of the next
        // frame. Applying here guarantees that step already sees the edit;
        // unordered in `Update`, a write that landed after this system ran
        // let one step use the old gravity — enough to set freshly spawned
        // bodies drifting under "zero" gravity (brief 2-5, bug 1).
        app.add_systems(
            PostUpdate,
            apply_sim_settings.run_if(resource_changed::<gradiance_domain::settings::SimSettings>),
        );
        app.init_resource::<hold::KinematicHold>();
        app.init_resource::<grab::MouseSpring>();
        app.init_resource::<grab::MouseTwist>();
        app.add_message::<fields::SetOrbitRequest>();
        app.register_type::<fields::SetOrbitRequest>();
        app.init_resource::<SubstepTrace>();
        app.add_systems(
            SubstepSchedule,
            record_substep_trace.run_if(substep_trace_enabled),
        );
        app.add_systems(OnEnter(GameState::Paused), pause_physics_clock);
        app.add_systems(OnExit(GameState::Paused), resume_physics_clock);
        // Owed `sim.step` steps (brief R-1): one per fixed tick, before
        // avian's step in `FixedPostUpdate`; the flag that says a step ran
        // this frame resets at the top of each frame.
        app.init_resource::<SimStep>();
        app.add_systems(First, clear_stepped_flag);
        app.add_systems(FixedFirst, advance_owed_step);
        app.add_systems(
            Update,
            (
                hold::apply_kinematic_hold,
                grab::apply_mouse_spring,
                grab::apply_mouse_twist,
                fields::sync_field_mass,
                fields::apply_field_forces.run_if(sim_running),
                fields::apply_plane_friction.run_if(sim_running),
                fields::set_in_orbit,
            ),
        );
        app.add_systems(
            PostUpdate,
            (
                body_sync::sync_colliders,
                body_sync::sync_collision_layers,
                joint_sync::guard_dangling_joints,
                joint_sync::sync_joints,
                joint_sync::retune_joints,
            )
                .chain()
                .in_set(BodySyncSet),
        );
        app.add_systems(Update, motor::drive_oscillating_motors);
        app.add_systems(
            FixedPostUpdate,
            joint_sync::break_overloaded_joints
                .in_set(PhysicsSystems::Last)
                .run_if(sim_running),
        );
    }
}

/// System set of the `Update` systems that change the transport (play,
/// pause, owed `sim.step` steps; the interaction layer's
/// `apply_sim_transport`). Systems that ask [`sim_running`] once per frame
/// and must agree with each other (the signal recorders) run after it, so a
/// step request that lands this frame is seen by all of them or by none
/// (brief R-2: a rollout's first row holds every signal).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransportSet;

/// System set for authored→engine body synchronization (runs in
/// `PostUpdate`, after commands mutated authored components in `Update`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodySyncSet;

/// Applies authored simulation settings to the engine.
fn apply_sim_settings(
    settings: Res<gradiance_domain::settings::SimSettings>,
    mut gravity: ResMut<Gravity>,
    mut time: ResMut<Time<Physics>>,
    mut fixed: ResMut<Time<Fixed>>,
    mut substeps: ResMut<SubstepCount>,
) {
    gravity.0 = settings.gravity.value();
    time.set_relative_speed(settings.speed.clamp(0.0, 10.0));
    substeps.0 = settings.substeps.clamp(1, 64);
    // The physics schedule runs on the fixed clock; this is the timestep.
    fixed.set_timestep_hz(f64::from(settings.timestep_hz.value().clamp(15.0, 240.0)));
}

/// Per-substep positions of dynamic bodies from the most recent physics
/// step — the substep debug view's data (feedback 8.3). Derived diagnostic:
/// rebuilt every step while `DebugSettings::show_substeps` is on, never
/// persisted (a stale trace is simply not drawn once the toggle is off).
#[derive(Resource, Default, Debug)]
pub struct SubstepTrace(pub Vec<Vec<Vec2>>);

/// Records dynamic-body positions inside avian's [`SubstepSchedule`]. The
/// trace holds exactly the last step's substeps: once it reaches
/// [`SubstepCount`] entries, the next substep starts a fresh step.
fn record_substep_trace(
    mut trace: ResMut<SubstepTrace>,
    substeps: Res<SubstepCount>,
    bodies: Query<(&Position, &RigidBody), With<gradiance_domain::Body>>,
) {
    if trace.0.len() >= substeps.0 as usize {
        trace.0.clear();
    }
    trace.0.push(
        bodies
            .iter()
            .filter(|(_, kind)| **kind == RigidBody::Dynamic)
            .map(|(pos, _)| Vec2::new(pos.x, pos.y))
            .collect(),
    );
}

/// Run condition: the substep debug view is enabled.
fn substep_trace_enabled(debug: Res<gradiance_domain::settings::DebugSettings>) -> bool {
    debug.show_substeps
}

/// Fixed steps a `sim.step` request still owes (brief R-1), advanced while
/// the simulation is paused: one per fixed tick, each exactly the fixed
/// timestep scaled by the sim speed, as a playing tick would be. Derived
/// transport state: never saved, never undone.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SimStep {
    remaining: u32,
    stepped: bool,
}

impl SimStep {
    /// Owes `n` more steps (a second request adds to the first).
    pub fn request(&mut self, n: u32) {
        self.remaining = self.remaining.saturating_add(n);
    }

    /// Drops the steps still owed (play and pause do).
    pub fn cancel(&mut self) {
        self.remaining = 0;
    }

    /// The steps still owed.
    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// Whether an owed step ran this frame.
    pub fn stepped_this_frame(&self) -> bool {
        self.stepped
    }

    /// Whether the simulation is stepping: steps are owed or one ran this
    /// frame.
    pub fn active(&self) -> bool {
        self.remaining > 0 || self.stepped
    }
}

/// Run condition: the simulation advances, by playing or by owed steps. What
/// gates the systems that act only on a running simulation (field forces,
/// joint breaking, signal recording, tracer trails), so a stepped run
/// behaves as the same span played.
pub fn sim_running(state: Res<State<GameState>>, step: Res<SimStep>) -> bool {
    *state.get() == GameState::Playing || step.active()
}

fn clear_stepped_flag(mut step: ResMut<SimStep>) {
    if step.stepped {
        step.stepped = false;
    }
}

/// Advances the paused physics clock by one fixed step when one is owed.
/// avian steps a paused clock that was advanced by hand (its
/// `run_physics_schedule` runs the schedule on any non-zero delta), so the
/// step is avian's own, with the substep clock set as a playing tick sets it.
fn advance_owed_step(
    mut step: ResMut<SimStep>,
    mut time: ResMut<Time<Physics>>,
    mut substep_time: ResMut<Time<Substeps>>,
    fixed: Res<Time<Fixed>>,
    substeps: Res<SubstepCount>,
) {
    if step.remaining == 0 || !time.is_paused() {
        return;
    }
    let delta = fixed.timestep().mul_f64(time.relative_speed_f64());
    time.advance_by(delta);
    substep_time.advance_by(delta.div_f64(f64::from(substeps.0.max(1))));
    step.remaining -= 1;
    step.stepped = true;
}

fn pause_physics_clock(mut time: ResMut<Time<Physics>>) {
    time.pause();
}

fn resume_physics_clock(mut time: ResMut<Time<Physics>>) {
    time.unpause();
}
