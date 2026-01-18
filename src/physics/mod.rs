//! Physics configuration and integration for Gradiance.
//!
//! This module configures the [Rapier](https://rapier.rs) physics engine.

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

        app.insert_resource(RapierConfiguration {
            gravity: Vec2::new(0.0, -1000.0),
            // Explicitly set other fields or use struct update syntax if Default is implemented for parts?
            // RapierConfiguration might not impl Default in this version?
            // Actually it should. Let's try `..RapierConfiguration::default()`? Or `..Default::default()`?
            // The error said `RapierConfiguration: Default` is not satisfied.
            // This means we must construct it manually or it doesn't impl Default.
            // checking docs for 0.27: It usually does. Maybe `bevy_rapier2d::plugin::RapierConfiguration`.
            // Let's explicitly set fields if needed.
            physics_pipeline_active: true,
            query_pipeline_active: true,
            timestep_mode: TimestepMode::Variable {
                max_dt: 1.0 / 60.0,
                time_scale: 1.0,
                substeps: 1,
            },
            scaled_shape_subdivision: 10,
            force_update_from_transform_changes: false,
        });

        app.add_systems(Update, pause_physics_system);

        app.add_plugins(constraints::ConstraintsPlugin);
    }
}

fn pause_physics_system(mut config: ResMut<RapierConfiguration>, time: Res<Time<Virtual>>) {
    config.physics_pipeline_active = !time.is_paused();
}
