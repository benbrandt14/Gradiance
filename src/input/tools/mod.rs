use crate::prelude::*;

pub mod box_tool;
pub mod circle_tool;

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((box_tool::BoxToolPlugin, circle_tool::CircleToolPlugin));
    }
}

pub trait Tool {
    fn name(&self) -> &str;
}
