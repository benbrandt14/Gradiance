//! Implementations of specific tools.
//!
//! Includes logic for Select, Box, Circle, Polygon, and Drag tools.

use crate::input::ToolState;
use crate::prelude::*;

pub mod box_tool;
pub mod circle_tool;
pub mod connector;
pub mod drag_tool;
pub mod ground_tool;
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
            ground_tool::GroundToolPlugin,
        ));
    }
}

/// Common trait for tool behavior (optional, currently unused structure).
pub trait Tool {
    /// Returns the name of the tool.
    fn name(&self) -> &str;
}

/// Defines the interaction paradigm of a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolBehavior {
    /// Tool creates or acts via a single click (or press-release cycle).
    /// Examples: Spawn at cursor (if implemented), Delete (click to delete).
    SingleClick,
    /// Tool operates via dragging (Press -> Move -> Release).
    /// Examples: Box, Circle, Polygon, Drag (Move).
    Drag,
    /// Tool specifically connects two points/entities (Press -> Drag -> Release).
    /// Examples: Connector (Joints).
    /// Note: Select tool is Hybrid (Click to select, Drag to move/box-select).
    ClickDragRelease,
    /// Tool has complex or mixed behavior.
    /// Examples: Select, Ground (Draws until released).
    Hybrid,
}

impl ToolState {
    /// Returns the interaction behavior for this tool state.
    pub fn get_behavior(&self) -> ToolBehavior {
        match self {
            ToolState::Select => ToolBehavior::Hybrid, // Click select, Drag move, Drag box
            ToolState::Box | ToolState::Circle | ToolState::Polygon => ToolBehavior::Drag,
            ToolState::Ground => ToolBehavior::Hybrid, // Drag to draw, but specific logic
            ToolState::RevoluteJoint
            | ToolState::PrismaticJoint
            | ToolState::SpringJoint
            | ToolState::RopeJoint
            | ToolState::Weld => ToolBehavior::ClickDragRelease,
            ToolState::Drag => ToolBehavior::Drag,
            ToolState::Cut => ToolBehavior::Drag, // Assumed drag to cut
            ToolState::Sketch => ToolBehavior::Drag, // Assumed drag to sketch
        }
    }
}
