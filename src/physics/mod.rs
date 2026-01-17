use crate::prelude::*;
use bevy::math::DVec2;

pub mod config;
pub mod constraints;

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Avian setup
        app.add_plugins(PhysicsPlugins::default());

        // Spec: Set SubstepCount to roughly 12-16.
        // Higher substeps increase stability for complex constraints (gears, chains).
        app.insert_resource(SubstepCount(12));

        // Spec: Gravity (standard)
        // Note: Avian with f64 requires DVec2
        app.insert_resource(Gravity(DVec2::new(0.0, -9.81)));

        // Spec: Custom constraints will be added here
        app.add_plugins(constraints::ConstraintsPlugin);
    }
}
