#![deny(missing_docs)]
// #![deny(clippy::missing_docs_in_private_items)]

//! # Gradiance
//!
//! A sloppy open-source 2D physics sandbox inspired by **Algodoo**, built in **Rust** using the [Bevy](https://bevyengine.org/) game engine and [Rapier](https://rapier.rs/) physics.
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

use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
// use bevy_mod_picking::DefaultPickingPlugins;
// use bevy_mod_picking::backends::rapier::RapierBackend;

/// System sets for ordering execution.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSystemSet {
    /// UI systems (drawing panels, handling clicks).
    Ui,
    /// Tool logic (checking pointer, creating shapes).
    Tools,
}

/// The primary plugin for the Gradiance game.
///
/// This plugin initializes all sub-systems including physics, geometry, input, UI, and scripting.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, GameSystemSet::Tools.after(GameSystemSet::Ui));

        app.add_plugins((
            ShapePlugin,
            // DefaultPickingPlugins,
            // RapierBackend,
            physics::PhysicsPlugin,
            geometry::GeometryPlugin,
            input::InputPlugin,
            ui::UiPlugin,
            // scripting::ScriptingPlugin,
        ))
        .add_systems(Startup, setup_camera);
    }
}

/// Spawns the main 2D camera.
fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
