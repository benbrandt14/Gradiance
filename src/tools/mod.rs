pub mod box_tool;
pub mod circle_tool;
pub mod move_tool;

use bevy::prelude::*;

#[derive(States, Default, Debug, Clone, Eq, PartialEq, Hash)]
pub enum ToolState {
    #[default]
    Move,
    Box,
    Circle,
    Hinge,
    Spring,
}

pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<ToolState>();
        app.add_plugins((
            box_tool::BoxToolPlugin,
            circle_tool::CircleToolPlugin,
            move_tool::MoveToolPlugin,
        ));
    }
}
