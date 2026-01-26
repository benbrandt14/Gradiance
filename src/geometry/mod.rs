//! Geometry processing and rendering.
//!
//! This module handles vector graphics rendering using `bevy_prototype_lyon` and
//! Constructive Solid Geometry (CSG) operations using `clipper2`.

use crate::prelude::*;

pub mod csg;
/// 2.5D mesh extrusion logic.
pub mod extrusion;

/// Plugin for geometry and vector rendering.
///
/// Initializes the `bevy_prototype_lyon` ShapePlugin for drawing shapes.
pub struct GeometryPlugin;

impl Plugin for GeometryPlugin {
    fn build(&self, app: &mut App) {
        // Lyon setup for vector rendering is now handled via ExtrusionPlugin
        // app.add_plugins(ShapePlugin);
        app.add_plugins(extrusion::ExtrusionPlugin);
    }
}
