//! Geometry processing and rendering.
//!
//! This module handles Constructive Solid Geometry (CSG) operations using `clipper2`.

use crate::prelude::*;

pub mod csg;

/// Plugin for geometry.
pub struct GeometryPlugin;

impl Plugin for GeometryPlugin {
    fn build(&self, _app: &mut App) {
    }
}
