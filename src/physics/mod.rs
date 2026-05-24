//! Physics configuration and integration for Gradiance.
//!
//! This module configures the Rapier physics engine.

use crate::prelude::*;
// Ensure DebugRenderContext is available.
use bevy_rapier2d::render::DebugRenderContext;

pub mod config;
pub mod constraints;
pub mod controllers;
pub mod floor;

const PIXELS_PER_METER: f32 = 100.0;
const GRAVITY: Vec2 = Vec2::new(0.0, -1000.0);

/// Plugin that configures the physics simulation.
pub struct GradiancePhysicsPlugin;

impl Plugin for GradiancePhysicsPlugin {
    fn build(&self, app: &mut App) {
        // TODO: Bound physics systems with run conditions (e.g., `run_if(in_state(GameState::Playing))`)
        // to prevent physics from running when paused or in menus.

        // Rapier setup
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(
            PIXELS_PER_METER,
        ))
        .add_plugins(RapierDebugRenderPlugin::default());

        // Configure Gravity
        app.add_systems(Startup, (configure_gravity, disable_debug_render));

        // Spec: Custom constraints will be added here
        app.add_plugins(constraints::ConstraintsPlugin);

        // Setup Floor
        app.add_plugins(floor::FloorPlugin);

        app.add_systems(Update, controllers::motor_controller_system);
    }
}

fn configure_gravity(mut config: Query<&mut RapierConfiguration>) {
    for mut config in &mut config {
        config.gravity = GRAVITY;
    }
}

fn disable_debug_render(mut context: ResMut<DebugRenderContext>) {
    context.enabled = false;
}
