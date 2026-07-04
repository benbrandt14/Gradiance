//! The single choke point turning intents into applied commands.

use crate::command::intent::{
    CommitTransformIntent, DeleteIntent, DuplicateIntent, RedoIntent, SpawnBodyIntent, UndoIntent,
};
use crate::command::spawn::{DeleteCommand, DuplicateCommand, SpawnBodyCommand};
use crate::command::transform_cmd::MoveRotateCommand;
use crate::command::{CommandStack, GameCommand};
use bevy::prelude::*;

fn drain<M: Message>(world: &mut World) -> Vec<M> {
    world
        .get_resource_mut::<Messages<M>>()
        .map(|mut messages| messages.drain().collect())
        .unwrap_or_default()
}

/// Drains all pending intents, builds commands, and applies them through
/// the [`CommandStack`]. This is the **only** code that touches the stack.
///
/// Failed commands log a warning and are not recorded; the intent is
/// simply consumed.
pub fn dispatch_intents(world: &mut World) {
    let mut commands: Vec<Box<dyn GameCommand>> = Vec::new();

    for intent in drain::<SpawnBodyIntent>(world) {
        commands.push(Box::new(SpawnBodyCommand {
            record: intent.record,
        }));
    }
    for intent in drain::<CommitTransformIntent>(world) {
        commands.push(Box::new(MoveRotateCommand {
            changes: intent.changes,
        }));
    }
    for intent in drain::<DuplicateIntent>(world) {
        commands.push(Box::new(DuplicateCommand::new(
            intent.sources,
            intent.offset,
        )));
    }
    for intent in drain::<DeleteIntent>(world) {
        commands.push(Box::new(DeleteCommand::new(intent.targets)));
    }

    let undos = drain::<UndoIntent>(world).len();
    let redos = drain::<RedoIntent>(world).len();

    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
        for command in commands {
            // Errors are already logged by push_apply; a failed intent is
            // consumed without corrupting history.
            let _ = stack.push_apply(command, world);
        }
        for _ in 0..undos {
            stack.undo(world);
        }
        for _ in 0..redos {
            stack.redo(world);
        }
    });
}
