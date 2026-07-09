//! The physics systems: collider/joint derivation and the read facade.
//!
//! Post de-adapter (`docs/physics-deadapter-decision.md`) authored physics
//! state *is* avian components on the entity; this module no longer translates
//! a mirror. It still owns the genuine derivations — `ShapeDef` → `Collider`
//! and `LayerMask32` → `CollisionLayers` ([`body_sync`]) — and the
//! [`queries`] read facade. avian is used directly here and elsewhere.
//!
//! Ground planes need no special support here: the ground tool authors an
//! ordinary `Static` body with a large box shape.

pub mod body_sync;
pub mod grab;
pub mod hold;
pub mod joint_sync;
pub mod motor;
pub mod queries;

use crate::core::constants::{GRAVITY, PIXELS_PER_METER};
use crate::core::states::GameState;
use avian2d::prelude::*;
use bevy::prelude::*;

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
                .with_length_unit(PIXELS_PER_METER)
                .build()
                .disable::<IslandPlugin>(),
        );
        // The collider picking backend needs bevy's core picking plugin
        // (present under DefaultPlugins, absent in headless test apps).
        if app.is_plugin_added::<bevy::picking::PickingPlugin>() {
            app.add_plugins(PhysicsPickingPlugin);
        }
        app.insert_resource(Gravity(GRAVITY));
        app.init_resource::<crate::domain::settings::SimSettings>();
        // Reflected config seam for scripting (see script-lisp-decision.md).
        app.register_type::<crate::domain::settings::SimSettings>();
        app.add_systems(
            Update,
            apply_sim_settings.run_if(resource_changed::<crate::domain::settings::SimSettings>),
        );
        app.init_resource::<hold::KinematicHold>();
        app.init_resource::<grab::MouseSpring>();
        app.add_systems(OnEnter(GameState::Paused), pause_physics_clock);
        app.add_systems(OnExit(GameState::Paused), resume_physics_clock);
        app.add_systems(
            Update,
            (hold::apply_kinematic_hold, grab::apply_mouse_spring),
        );
        app.add_systems(
            PostUpdate,
            (
                body_sync::sync_colliders,
                body_sync::sync_collision_layers,
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
    settings: Res<crate::domain::settings::SimSettings>,
    mut gravity: ResMut<Gravity>,
    mut time: ResMut<Time<Physics>>,
    mut substeps: ResMut<SubstepCount>,
) {
    gravity.0 = settings.gravity;
    time.set_relative_speed(settings.speed.clamp(0.0, 10.0));
    substeps.0 = settings.substeps.clamp(1, 64);
}

fn pause_physics_clock(mut time: ResMut<Time<Physics>>) {
    time.pause();
}

fn resume_physics_clock(mut time: ResMut<Time<Physics>>) {
    time.unpause();
}
