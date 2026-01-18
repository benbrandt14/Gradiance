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

use crate::prelude::*;
use bevy::math::DVec2;
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
    // Vertices such that the top edge is at y=0
    let points = vec![
        Vec2::new(-w, -depth),
        Vec2::new(w, -depth),
        Vec2::new(w, 0.0),
        Vec2::new(-w, 0.0),
    ];

    let shape = shapes::Polygon {
        points,
        closed: true,
    };

    commands.spawn((
        ShapeBuilder::with(&shape)
            .fill(Color::srgb(0.2, 0.2, 0.2))
            .stroke(Stroke::new(Color::BLACK, 1.0))
            .build(),
        RigidBody::Static,
        // Infinite half-space collider pointing up (Normal = Y)
        Collider::half_space(DVec2::new(0.0, 1.0)),
        Friction::new(0.5),
        Restitution::new(0.0),
        GroundPlane,
        Transform::from_xyz(0.0, -200.0, 0.0),
    ));
}
