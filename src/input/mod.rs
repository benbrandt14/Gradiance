//! Input handling and tool state management.
//!
//! This module coordinates user input, mouse picking, tool selection (Select, Box, Drag, etc.),
//! and the state machine that governs tool behavior.

use crate::prelude::*;
// use bevy_mod_picking::DefaultPickingPlugins;
// Commented out due to version mismatch (bevy_mod_picking 0.20.1 targets Bevy 0.14).
// Bevy 0.17+ has built-in picking.

pub mod camera_controller;
pub mod cursor;
pub mod editable;
pub mod selection;
pub mod tools;

/// Plugin for Input and Tools.
///
/// Initializes the cursor, selection, tool state, and specific tool plugins.
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        // Picking setup
        // app.add_plugins(DefaultPickingPlugins);
        // Using Bevy's built-in picking (included in DefaultPlugins)

        // Cursor
        app.init_resource::<cursor::CursorWorldPos>();
        app.add_systems(PreUpdate, cursor::update_cursor_pos);

        // Camera Controller
        app.add_plugins(camera_controller::CameraControllerPlugin);

        // Selection
        app.add_plugins(selection::SelectionPlugin);

        // Tool state
        app.init_state::<ToolState>();

        // Tools
        app.add_plugins(tools::ToolsPlugin);

        // Editable shapes
        app.add_plugins(editable::EditablePlugin);

        // Z-Index management
        app.init_resource::<ZIndex>();

        // Global shortcuts
        app.add_systems(Update, toggle_pause);
    }
}

/// Resource to manage Z-index to prevent Z-fighting.
#[derive(Resource, Default)]
pub struct ZIndex(pub f32);

impl ZIndex {
    /// Get the next Z-index value and increment.
    pub fn next(&mut self) -> f32 {
        self.0 += 0.001;
        self.0
    }
}

fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut virtual_time: ResMut<Time<Virtual>>) {
    if keys.just_pressed(KeyCode::Space) {
        if virtual_time.is_paused() {
            virtual_time.unpause();
        } else {
            virtual_time.pause();
        }
    }
}

/// The active tool state.
///
/// Determines which tool logic is active in the `Update` loop.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ToolState {
    /// Select and inspect objects.
    #[default]
    Select,
    /// Drag and throw objects.
    Drag,
    /// Cut geometry (Laser/Knife).
    Cut,
    /// Freehand sketch tool.
    Sketch,
    /// Create box shapes.
    Box,
    /// Create circle shapes.
    Circle,
    /// Create polygon shapes.
    Polygon,
    /// Create revolute joints (Axles).
    RevoluteJoint,
    /// Create fixed joints (Welds).
    Weld,
    // Add more as needed
}
