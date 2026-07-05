//! The single choke point turning intents into applied commands.

use crate::command::array_cmd::ArrayCommand;
use crate::command::group_cmd::{GroupCommand, UngroupCommand};
use crate::command::intent::LoadSceneIntent;
use crate::command::intent::{
    ArrayIntent, CommitTransformIntent, DeleteIntent, DuplicateIntent, GroupIntent,
    PropertyEditIntent, RedoIntent, ScaleIntent, SpawnBodyIntent, SpawnJointIntent, UndoIntent,
    UngroupIntent,
};
use crate::command::joint_cmd::SpawnJointCommand;
use crate::command::property::SetPropertyCommand;
use crate::command::scale_cmd::ScaleCommand;
use crate::command::scene_cmd::LoadSceneCommand;
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
    for intent in drain::<SpawnJointIntent>(world) {
        commands.push(Box::new(SpawnJointCommand {
            record: intent.record,
        }));
    }
    for intent in drain::<LoadSceneIntent>(world) {
        commands.push(Box::new(LoadSceneCommand::new(intent.scene)));
    }
    for intent in drain::<PropertyEditIntent>(world) {
        commands.push(Box::new(SetPropertyCommand {
            changes: intent.changes,
        }));
    }
    for intent in drain::<GroupIntent>(world) {
        commands.push(Box::new(GroupCommand::new(intent.targets)));
    }
    for intent in drain::<UngroupIntent>(world) {
        commands.push(Box::new(UngroupCommand::new(intent.targets)));
    }
    for intent in drain::<ScaleIntent>(world) {
        commands.push(Box::new(ScaleCommand::new(
            intent.targets,
            intent.pivot,
            intent.frame_rot,
            intent.factors,
        )));
    }
    for intent in drain::<ArrayIntent>(world) {
        commands.push(Box::new(ArrayCommand::new(
            intent.sources,
            intent.count,
            intent.mode,
        )));
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
