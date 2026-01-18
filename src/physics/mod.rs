//! Physics configuration and integration for Gradiance.
//!
//! This module configures the [Avian](https://github.com/Jondolf/avian) physics engine with settings
//! optimized for mechanical simulation, such as high precision (`f64`) and increased substeps.

use crate::prelude::*;
use bevy::math::DVec2;

pub mod config;
pub mod constraints;

/// Plugin that configures the physics simulation.
///
/// This plugin initializes Avian, sets gravity, configures substeps for stability,
/// and registers custom constraints.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Avian setup
        app.add_plugins(PhysicsPlugins::default());

        // Spec: Set SubstepCount to roughly 12-16.
        // Higher substeps increase stability for complex constraints (gears, chains).
        // Reduced to 8 to improve performance/sluggishness while maintaining some stability.
        app.insert_resource(SubstepCount(8));

        // Spec: Gravity (standard)
        // Note: Avian with f64 requires DVec2
        // Increased to -1000.0 to match pixel coordinate scale (approx 100px = 1m feel).
        app.insert_resource(Gravity(DVec2::new(0.0, -1000.0)));

        // Spec: Custom constraints will be added here
        app.add_plugins(constraints::ConstraintsPlugin);

        // Pause physics when Virtual time is paused.
        app.configure_sets(
            FixedUpdate,
            PhysicsSet::StepSimulation.run_if(|time: Res<Time<Virtual>>| !time.is_paused()),
        );
    }
}
