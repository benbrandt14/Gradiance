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
// use bevy_prototype_lyon::prelude::*;
// use bevy_mod_picking::DefaultPickingPlugins;
// use bevy_mod_picking::backends::rapier::RapierBackend;

/// The primary plugin for the Gradiance game.
///
/// This plugin initializes all sub-systems including physics, geometry, input, UI, and scripting.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // DefaultPickingPlugins,
            // RapierBackend,
            physics::PhysicsPlugin,
            geometry::GeometryPlugin,
            input::InputPlugin,
            ui::UiPlugin,
            // scripting::ScriptingPlugin,
        ))
        .add_systems(Startup, (setup_camera, setup_ground));
    }
}

/// Spawns the main 2D camera.
fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Marker component for the infinite ground plane.
#[derive(Component)]
pub struct GroundPlane;

/// Spawns a static ground plane.
fn setup_ground(mut commands: Commands) {
    // Visual representation (very wide rectangle to simulate infinity)
    let w = 100_000.0;
    let depth = 1000.0; // Deep enough to look like "ground"
    
    // Centered rectangle for visual and collider alignment
    /*
    let shape = shapes::Rectangle {
        extents: Vec2::new(w * 2.0, depth),
        origin: shapes::RectangleOrigin::Center,
        ..default()
    };
    */

    commands.spawn((
        /*
        ShapeBundle {
            path: GeometryBuilder::build_as(&shape),
            ..default()
        },
        Fill::color(Color::srgb(0.2, 0.2, 0.2)),
        Stroke::new(Color::BLACK, 1.0),
        */
        // Use Sprite instead of Lyon shape for now? Or just invisible physics.
        // For simplicity, just invisible physics + maybe a sprite if I had one.
        // I'll leave it invisible or use a Gizmo in a system.
        RigidBody::Fixed,
        // Rapier 2D uses cuboid with half-extents.
        Collider::cuboid(w, depth / 2.0),
        Friction::coefficient(0.5),
        Restitution::coefficient(0.0),
        GroundPlane,
        // Top edge at -200.0. Center is at -200 - depth/2.
        Transform::from_xyz(0.0, -200.0 - depth / 2.0, 0.0),
    ));
}
