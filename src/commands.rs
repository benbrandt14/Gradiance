//! Command pattern implementation for Undo/Redo functionality.

use bevy::prelude::*;
use std::collections::VecDeque;

/// A command that can be executed and undone.
pub trait GameCommand: Send + Sync + 'static {
    /// Executes the command.
    fn execute(&mut self, world: &mut World);

    /// Undoes the command.
    fn undo(&mut self, world: &mut World);
}

/// Resource that manages the stack of executed commands.
#[derive(Resource, Default)]
pub struct CommandStack {
    /// Stack of commands that can be undone.
    undo_stack: VecDeque<Box<dyn GameCommand>>,
    /// Stack of commands that can be redone.
    redo_stack: VecDeque<Box<dyn GameCommand>>,
}

impl CommandStack {
    /// Pushes a new command to the stack and executes it.
    /// Clears the redo stack.
    pub fn push(&mut self, mut command: Box<dyn GameCommand>, world: &mut World) {
        command.execute(world);
        self.undo_stack.push_back(command);
        self.redo_stack.clear();
    }

    /// Undoes the last command.
    pub fn undo(&mut self, world: &mut World) {
        if let Some(mut command) = self.undo_stack.pop_back() {
            command.undo(world);
            self.redo_stack.push_back(command);
        }
    }

    /// Redoes the last undone command.
    pub fn redo(&mut self, world: &mut World) {
        if let Some(mut command) = self.redo_stack.pop_back() {
            command.execute(world);
            self.undo_stack.push_back(command);
        }
    }
}

/// Plugin to setup command infrastructure.
pub struct CommandsPlugin;

impl Plugin for CommandsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandStack>();
        app.add_systems(Update, handle_undo_redo_input);
    }
}

fn handle_undo_redo_input(
    world: &mut World,
) {
    let shift_pressed = world.resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ShiftLeft)
        || world.resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ShiftRight);

    let ctrl_pressed = world.resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ControlLeft)
        || world.resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ControlRight);

    let z_pressed = world.resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::KeyZ);
    let y_pressed = world.resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::KeyY);

    let undo = z_pressed && ctrl_pressed && !shift_pressed;

    // Ctrl+Y or Ctrl+Shift+Z for Redo
    let redo = (y_pressed && ctrl_pressed) || (z_pressed && ctrl_pressed && shift_pressed);

    if undo {
        world.resource_scope(|world, mut stack: Mut<CommandStack>| {
            stack.undo(world);
        });
    } else if redo {
        world.resource_scope(|world, mut stack: Mut<CommandStack>| {
            stack.redo(world);
        });
    }
}
