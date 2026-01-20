//! Physics configuration and integration for Gradiance.
//!
//! This module configures the Rapier physics engine.

use crate::prelude::*;

pub mod config;
pub mod constraints;
pub mod floor;

/// Plugin that configures the physics simulation.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Rapier setup
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
            .add_plugins(RapierDebugRenderPlugin::default());

        // Configure Physics (Gravity)
        app.add_systems(Startup, configure_physics);

        // Spec: Custom constraints will be added here
        app.add_plugins(constraints::ConstraintsPlugin);

        // Setup Floor
        app.add_plugins(floor::FloorPlugin);
    }
}

fn configure_physics(mut config: Query<&mut RapierConfiguration>) {
    for mut config in &mut config {
        config.gravity = Vec2::new(0.0, -1000.0);
    }
}
