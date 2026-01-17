use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;

pub mod csg;

pub struct GeometryPlugin;

impl Plugin for GeometryPlugin {
    fn build(&self, app: &mut App) {
        // Lyon setup for vector rendering
        app.add_plugins(ShapePlugin);
    }
}
