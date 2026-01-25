//! Global keyboard shortcuts.
//!
//! Handles global shortcuts like CTRL+A (Select All), CTRL+Z (Undo), CTRL+Y (Redo), and Space (Pause/Resume).

use crate::input::{
    ToolState, commands,
    selection::{Selection, SelectionFilter},
    tools::connector::Connector,
};
use crate::physics::floor::GroundPlane;
use crate::prelude::*;

/// Plugin for handling global shortcuts.
pub struct ShortcutsPlugin;

impl Plugin for ShortcutsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_undo_redo_input, toggle_pause, handle_ctrl_a),
        );
    }
}

fn handle_ctrl_a(
    mut selection: ResMut<Selection>,
    selection_filter: Res<SelectionFilter>,
    keys: Res<ButtonInput<KeyCode>>,
    selectable_query: Query<Entity, (With<Collider>, Without<GroundPlane>)>,
    connector_query: Query<&Connector>,
    mut next_tool_state: ResMut<NextState<ToolState>>,
    current_tool_state: Res<State<ToolState>>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if ctrl && keys.just_pressed(KeyCode::KeyA) {
        // Switch to Select tool if not already
        if *current_tool_state.get() != ToolState::Select {
            next_tool_state.set(ToolState::Select);
        }

        if !shift {
            selection.clear();
        }

        let mut count = 0;
        for entity in &selectable_query {
            let is_connector = connector_query.contains(entity);
            let pass = match *selection_filter {
                SelectionFilter::All => true,
                SelectionFilter::Shapes => !is_connector,
                SelectionFilter::Joints => is_connector,
            };
            if pass {
                selection.add(entity);
                count += 1;
            }
        }
        info!("Select All: Selected {} entities", count);
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
