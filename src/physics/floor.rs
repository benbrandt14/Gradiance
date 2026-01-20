//! Infinite Floor Plugin
//!
//! Handles the setup and rendering of the infinite ground plane.

use crate::prelude::*;

/// Plugin for the infinite floor.
pub struct FloorPlugin;

impl Plugin for FloorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_ground);
    }
}

/// Marker component for the infinite ground plane.
#[derive(Component)]
pub struct GroundPlane;

/// Spawns a static ground plane.
fn setup_ground(mut commands: Commands) {
    // Visual representation (very wide rectangle to simulate infinity)
    let w = 100_000.0;
    let depth = 1000.0; // Deep enough to look like "ground"

    commands.spawn((
        RigidBody::Fixed,
        // Rapier 2D uses cuboid with half-extents.
        Collider::cuboid(w, depth / 2.0),
        Friction::coefficient(0.5),
        Restitution::coefficient(0.0),
        GroundPlane,
        // Top edge at -200.0. Center is at -200 - depth/2.
        Transform::from_xyz(0.0, -200.0 - depth / 2.0, 0.0),
        Name::new("Ground"),
    ));
}
