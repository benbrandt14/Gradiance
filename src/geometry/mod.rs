//! Geometry processing and rendering.
//!
//! This module handles vector graphics rendering using `bevy_prototype_lyon` and
//! Constructive Solid Geometry (CSG) operations using `clipper2`.

use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;

pub mod csg;

/// Plugin for geometry and vector rendering.
///
/// Initializes the `bevy_prototype_lyon` ShapePlugin for drawing shapes.
pub struct GeometryPlugin;

impl Plugin for GeometryPlugin {
    fn build(&self, app: &mut App) {
        // Lyon setup for vector rendering
        app.add_plugins(ShapePlugin);
    }
}
