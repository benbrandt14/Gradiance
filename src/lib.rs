#![deny(missing_docs)]
// #![deny(clippy::missing_docs_in_private_items)]

//! # Gradiance
//!
//! A sloppy open-source 2D physics sandbox inspired by **Algodoo**, built in **Rust** using the [Bevy](https://bevyengine.org/) game engine and [Avian](https://github.com/Jondolf/avian) physics.
//!
//! ## Status
//! * **Documentation**: Enforced via `deny(missing_docs)`.
//! * **Ordering**: Visualized via `bevy_mod_debugdump`.
//!
//! //! ## Bevy Schedule Graph
//! ![Schedule Graph](doc/architecture.png)

pub mod geometry;
pub mod input;
pub mod physics;
pub mod prelude;
pub mod scripting;
pub mod ui;

use bevy::prelude::*;

/// The primary plugin for the Gradiance game.
///
/// This plugin initializes all sub-systems including physics, geometry, input, UI, and scripting.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            physics::PhysicsPlugin,
            geometry::GeometryPlugin,
            input::InputPlugin,
            ui::UiPlugin,
            scripting::ScriptingPlugin,
        ))
        .add_systems(Startup, setup_camera);
    }
}

/// Spawns the main 2D camera.
fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}
