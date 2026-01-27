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
// use bevy_mod_picking::DefaultPickingPlugins;
// use bevy_mod_picking::backends::rapier::RapierBackend;

/// The primary plugin for the Gradiance game.
///
/// This plugin initializes all sub-systems including physics, geometry, input, UI, and scripting.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // ShapePlugin, // Removed in favor of ExtrusionPlugin (via GeometryPlugin)
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

/// Spawns the main 3D camera for 2.5D visualization.
fn setup_camera(mut commands: Commands) {
    // Camera positioned further back to see all layers (Z=0 to Z=320)
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, -30.0, 500.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Also add a light so we can see the 3D meshes
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, -0.5, 0.0)),
    ));
}
