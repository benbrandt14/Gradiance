//! Input handling and tool state management.
//!
//! This module coordinates user input, mouse picking, tool selection (Select, Box, Drag, etc.),
//! and the state machine that governs tool behavior.

use crate::prelude::*;

pub mod camera_controller;
pub mod cursor;
pub mod editable;
pub mod event_handlers;
/// Events for tool interactions.
pub mod events;
pub mod selection;
pub mod tools;

/// Plugin for Input and Tools.
///
/// Initializes the cursor, selection, tool state, and specific tool plugins.
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        // Events
        app.add_event::<events::SpawnBoxEvent>()
            .add_event::<events::SpawnCircleEvent>()
            .add_event::<events::SpawnPolygonEvent>()
            .add_event::<events::SpawnGroundEvent>()
            .add_event::<events::SpawnJointEvent>()
            .add_event::<events::SpawnGearEvent>()
            .add_event::<events::SpawnPulleyEvent>()
            .add_event::<events::SpawnChainEvent>()
            .add_event::<events::ModifyTransformEvent>()
            .add_event::<events::ModifyPhysicsEvent>()
            .add_event::<events::ModifyShapeEvent>()
            .add_event::<events::ModifyRenderEvent>()
            .add_event::<events::ModifyAttractionEvent>()
            .add_event::<events::ModifyJointEvent>();

        // Event Handlers
        app.add_systems(
            Update,
            (
                event_handlers::handle_spawn_box,
                event_handlers::handle_spawn_circle,
                event_handlers::handle_spawn_polygon,
                event_handlers::handle_spawn_ground,
                event_handlers::handle_spawn_joint,
                event_handlers::handle_spawn_gear,
                event_handlers::handle_spawn_pulley,
                event_handlers::handle_spawn_chain,
                event_handlers::handle_modify_transform,
                event_handlers::handle_modify_physics,
                event_handlers::handle_modify_shape,
                event_handlers::handle_modify_render,
                event_handlers::handle_modify_attraction,
                event_handlers::handle_modify_joint,
            ),
        );

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
        app.add_systems(Update, (toggle_pause, log_tool_transitions));
    }
}

fn log_tool_transitions(mut events: EventReader<StateTransitionEvent<ToolState>>) {
    for event in events.read() {
        if let Some(state) = event.entered {
            info!("Tool Changed to: {:?}", state);
        }
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
    /// Create prismatic joints (Sliders).
    PrismaticJoint,
    /// Create spring joints (Distance with spring).
    SpringJoint,
    /// Create rope joints (Max distance).
    RopeJoint,
    /// Create infinite ground plane.
    Ground,
    // Add more as needed
}
