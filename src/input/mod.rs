//! Input handling and tool state management.
//!
//! This module coordinates user input, mouse picking, tool selection (Select, Box, Drag, etc.),
//! and the state machine that governs tool behavior.

use crate::prelude::*;
use crate::geometry::gear::GearProfile;
use crate::input::commands::{GameCommand, SpawnGearCommand};
use crate::physics::constraints::GearJoint;
// use bevy_mod_picking::DefaultPickingPlugins;
// Commented out due to version mismatch (bevy_mod_picking 0.20.1 targets Bevy 0.14).
// Bevy 0.17+ has built-in picking.

pub mod camera_controller;
pub mod commands;
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

        // Commands
        app.init_resource::<commands::CommandStack>();

        // Tool state
        app.init_state::<ToolState>();

        // Tools
        app.add_plugins(tools::ToolsPlugin);

        // Editable shapes
        app.add_plugins(editable::EditablePlugin);

        // Z-Index management
        app.init_resource::<ZIndex>();

        // Global shortcuts
        app.add_systems(
            Update,
            (toggle_pause, handle_undo_redo_input, log_tool_transitions, spawn_example_gear_train),
        );
    }
}

fn log_tool_transitions(mut events: EventReader<StateTransitionEvent<ToolState>>) {
    for event in events.read() {
        if let Some(state) = event.entered {
            info!("Tool Changed to: {:?}", state);
        }
    }
}

fn handle_undo_redo_input(world: &mut World) {
    let mut undo = false;
    let mut redo = false;

    if let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() {
        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

        if ctrl && keys.just_pressed(KeyCode::KeyZ) {
            if shift {
                redo = true;
            } else {
                undo = true;
            }
        }
        if ctrl && keys.just_pressed(KeyCode::KeyY) {
            redo = true;
        }
    }

    if undo {
        world.resource_scope(|world, mut stack: Mut<commands::CommandStack>| {
            stack.undo(world);
        });
    }

    if redo {
        world.resource_scope(|world, mut stack: Mut<commands::CommandStack>| {
            stack.redo(world);
        });
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
    /// Create infinite ground plane.
    Ground,
    /// Create gears.
    Gear,
    // Add more as needed
}

fn spawn_example_gear_train(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.pressed(KeyCode::ControlLeft) && keys.just_pressed(KeyCode::KeyG) {
        commands.queue(|world: &mut World| {
            let m = 10.0;
            let z_a = 20;
            let z_b = 40;
            let z_c = 10;

            let pos_a = Vec2::new(0.0, 0.0);
            let r_a = m * z_a as f32 / 2.0;

            let r_b = m * z_b as f32 / 2.0;
            let dist_ab = r_a + r_b;
            let pos_b = pos_a + Vec2::new(dist_ab, 0.0);

            let r_c = m * z_c as f32 / 2.0;
            let dist_bc = r_b + r_c;
            // Place C above B
            let pos_c = pos_b + Vec2::new(0.0, dist_bc);

            let mut cmd_a = SpawnGearCommand {
                position: pos_a,
                profile: GearProfile::new(m, z_a, 20.0),
                entity: None,
                pin_entity: None,
            };
            if let Err(e) = cmd_a.apply(world) { error!("{}", e); return; }
            let entity_a = cmd_a.entity.unwrap();

            let mut cmd_b = SpawnGearCommand {
                position: pos_b,
                profile: GearProfile::new(m, z_b, 20.0),
                entity: None,
                pin_entity: None,
            };
            if let Err(e) = cmd_b.apply(world) { error!("{}", e); return; }
            let entity_b = cmd_b.entity.unwrap();

            let mut cmd_c = SpawnGearCommand {
                position: pos_c,
                profile: GearProfile::new(m, z_c, 20.0),
                entity: None,
                pin_entity: None,
            };
            if let Err(e) = cmd_c.apply(world) { error!("{}", e); return; }
            let entity_c = cmd_c.entity.unwrap();

            // Add Joints
            world.spawn(GearJoint {
                entity_a,
                entity_b,
                ratio: (z_b as f64) / (z_a as f64),
            });

            world.spawn(GearJoint {
                entity_a: entity_b,
                entity_b: entity_c,
                ratio: (z_c as f64) / (z_b as f64),
            });

            info!("Spawned Example Gear Train (A:20t, B:40t, C:10t)");
        });
    }
}
