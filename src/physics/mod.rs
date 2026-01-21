//! Physics configuration and integration for Gradiance.
//!
//! This module configures the Rapier physics engine.

use crate::prelude::*;

pub mod config;
pub mod constraints;
pub mod floor;
pub mod layers;

/// Plugin that configures the physics simulation.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Create shared exclusion map
        let exclusion_map = layers::CollisionExclusionMap::default();
        let _hook = layers::ExclusionHook::new(exclusion_map.map.clone());

        app.insert_resource(exclusion_map);

        // Rapier setup
        // Note: Custom physics hooks integration is temporarily disabled due to API mismatch.
        // The exclusion hook is created but not registered.
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
            .add_plugins(RapierDebugRenderPlugin::default());

        // Configure Gravity
        app.add_systems(Startup, configure_gravity);

        // Sync systems
        app.add_systems(Update, (layers::sync_physics_layers, layers::sync_collision_exclusions));

        // Spec: Custom constraints will be added here
        app.add_plugins(constraints::ConstraintsPlugin);

        // Setup Floor
        app.add_plugins(floor::FloorPlugin);
    }
}

fn configure_gravity(mut config: Query<&mut RapierConfiguration>) {
    for mut config in &mut config {
        config.gravity = Vec2::new(0.0, -1000.0);
    }
}
