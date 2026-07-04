//! The physics seam — the **only** module allowed to reference `avian2d`.
//!
//! Authored domain components are translated into engine components by
//! `Changed<>`-driven sync systems ([`body_sync`]); everything outside this
//! module interacts with physics exclusively through domain components and
//! the [`queries`] facade. Swapping physics engines means rewriting this
//! module and nothing else.
//!
//! Ground planes need no special support here: the ground tool authors an
//! ordinary `Static` body with a large box shape.

pub mod body_sync;
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
        app.add_plugins(PhysicsPlugins::default().with_length_unit(PIXELS_PER_METER));
        app.add_plugins(PhysicsPickingPlugin);
        app.insert_resource(Gravity(GRAVITY));
        app.add_systems(OnEnter(GameState::Paused), pause_physics_clock);
        app.add_systems(OnExit(GameState::Paused), resume_physics_clock);
        app.add_systems(
            PostUpdate,
            (
                body_sync::sync_colliders,
                body_sync::sync_rigid_bodies,
                body_sync::sync_collision_layers,
            )
                .in_set(BodySyncSet),
        );
    }
}

/// System set for authored→engine body synchronization (runs in
/// `PostUpdate`, after commands mutated authored components in `Update`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodySyncSet;

fn pause_physics_clock(mut time: ResMut<Time<Physics>>) {
    time.pause();
}

fn resume_physics_clock(mut time: ResMut<Time<Physics>>) {
    time.unpause();
}
