//! Implementations of specific tools.
//!
//! Includes logic for Select, Box, Circle, Polygon, and Drag tools.

use crate::prelude::*;

pub mod box_tool;
pub mod circle_tool;
pub mod connector;
pub mod drag_tool;
pub mod polygon_tool;
pub mod select_tool;
pub mod utils;

/// Plugin that registers all tool sub-plugins.
pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            box_tool::BoxToolPlugin,
            circle_tool::CircleToolPlugin,
            select_tool::SelectToolPlugin,
            polygon_tool::PolygonToolPlugin,
            drag_tool::DragToolPlugin,
            connector::ConnectorToolPlugin,
        ));
    }
}

/// Common trait for tool behavior (optional, currently unused structure).
pub trait Tool {
    /// Returns the name of the tool.
    fn name(&self) -> &str;
}
