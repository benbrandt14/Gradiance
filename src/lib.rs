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

/// The primary plugin for the Gradiance game.
///
/// This plugin initializes all sub-systems including physics, geometry, input, UI, and scripting.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
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

/// Spawns the main 3D camera and light.
fn setup_camera(mut commands: Commands) {
    // Camera3d with Perspective
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, -30.0, 60.0).looking_at(Vec3::ZERO, Vec3::Z),
        Projection::Perspective(PerspectiveProjection::default()),
    ));

    // Directional Light for 3D meshes
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(50.0, -50.0, 100.0).looking_at(Vec3::ZERO, Vec3::Z),
    ));
}
