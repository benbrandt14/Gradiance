use crate::prelude::*;
// use bevy_mod_picking::DefaultPickingPlugins;
// Commented out due to version mismatch (bevy_mod_picking 0.20.1 targets Bevy 0.14).
// Bevy 0.17+ has built-in picking.

pub mod cursor;
pub mod selection;
pub mod tools;
pub mod editable;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        // Picking setup
        // app.add_plugins(DefaultPickingPlugins);
        // Using Bevy's built-in picking (included in DefaultPlugins)

        // Cursor
        app.init_resource::<cursor::CursorWorldPos>();
        app.add_systems(PreUpdate, cursor::update_cursor_pos);

        // Selection
        app.add_plugins(selection::SelectionPlugin);

        // Tool state
        app.init_state::<ToolState>();

        // Tools
        app.add_plugins(tools::ToolsPlugin);

        // Editable shapes
        app.add_plugins(editable::EditablePlugin);
    }
}

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ToolState {
    #[default]
    Select,
    Drag,
    Cut,
    Sketch,
    Box,
    Circle,
    Polygon,
    // Add more as needed
}
