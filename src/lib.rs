#![deny(missing_docs)]
// #![deny(clippy::missing_docs_in_private_items)]

//! # Gradiance
//!
//! A sloppy open-source 2D physics sandbox inspired by **Algodoo**, built in **Rust** using the [Bevy](https://bevyengine.org/) game engine and [Rapier](https://rapier.rs) physics.
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
        .add_systems(Startup, (setup_camera, setup_ground));
    }
}

/// Spawns the main 2D camera.
fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

/// Marker component for the infinite ground plane.
#[derive(Component)]
pub struct GroundPlane;

/// Spawns a static ground plane.
fn setup_ground(mut commands: Commands) {
    // Visual representation (very wide rectangle to simulate infinity)
    let w = 100_000.0;
    // Vertices such that the top edge is at y=0
    let points = vec![
        Vec2::new(-w, -500.0),
        Vec2::new(w, -500.0),
        Vec2::new(w, 500.0),
        Vec2::new(-w, 500.0),
    ];

    let shape = shapes::Polygon {
        points,
        closed: true,
    };

    commands.spawn((
        ShapeBundle {
            path: GeometryBuilder::build_as(&shape),
            ..default()
        },
        Fill::color(Color::srgb(0.2, 0.2, 0.2)),
        Stroke::new(Color::BLACK, 1.0),
        RigidBody::Fixed,
        // Rapier Cuboid
        Collider::cuboid(100000.0, 500.0),
        Friction::coefficient(0.5),
        Restitution::coefficient(0.0),
        GroundPlane,
        Transform::from_xyz(0.0, -700.0, 0.0),
        GlobalTransform::default(),
        VisibilityBundle::default(),
    ));
}
