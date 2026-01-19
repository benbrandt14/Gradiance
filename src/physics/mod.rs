//! Physics configuration and integration for Gradiance.
//!
//! This module configures the Rapier physics engine.

use crate::prelude::*;

pub mod config;
pub mod constraints;

/// Plugin that configures the physics simulation.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Rapier setup
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
            .add_plugins(RapierDebugRenderPlugin::default());

        // Configure Gravity
        // TODO: Fix RapierConfiguration resource issue (version conflict?)
        // The default gravity in Rapier is usually (0.0, -9.81), but pixel scale affects this.
        // We wanted -1000.0.
        // If we can't insert the resource, we accept default or find another way (e.g. modify it in a startup system).
        /*
        app.insert_resource(RapierConfiguration {
            gravity: Vec2::new(0.0, -1000.0),
            ..default()
        });
        */
        // app.add_systems(Startup, configure_gravity);

        // Spec: Custom constraints will be added here
        app.add_plugins(constraints::ConstraintsPlugin);
    }
}

// fn configure_gravity(mut rapier_config: ResMut<RapierConfiguration>) {
//     rapier_config.gravity = Vec2::new(0.0, -1000.0);
// }
