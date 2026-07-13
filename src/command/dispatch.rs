//! The single choke point turning intents into applied commands.

use crate::command::array_cmd::ArrayCommand;
use crate::command::cut_cmd::CutCommand;
use crate::command::group_cmd::{GroupCommand, UngroupCommand};
use crate::command::intent::LoadSceneIntent;
use crate::command::intent::{
    ArrayIntent, CommitTransformIntent, CutIntent, DeleteIntent, DeleteJointIntent,
    DuplicateIntent, GroupIntent, MergeIntent, PropertyEditIntent, RedoIntent, ScaleIntent,
    SpawnBodyIntent, SpawnJointIntent, UndoIntent, UngroupIntent,
};
use crate::command::joint_cmd::{DeleteJointCommand, SpawnJointCommand};
use crate::command::merge_cmd::MergeCommand;
use crate::command::property::PropertyEditCommand;
use crate::command::scale_cmd::ScaleCommand;
use crate::command::scene_cmd::LoadSceneCommand;
use crate::command::spawn::{DeleteCommand, DuplicateCommand, SpawnBodyCommand};
use crate::command::transform_cmd::CommitTransformCommand;
use crate::command::{CommandStack, GameCommand};
use bevy::prelude::*;

fn drain<M: Message + Reflect + bevy::reflect::TypePath>(world: &mut World) -> Vec<M> {
    let drained: Vec<M> = world
        .get_resource_mut::<Messages<M>>()
        .map(|mut messages| messages.drain().collect())
        .unwrap_or_default();
    // Flight recorder (dev): every drained intent is recorded via reflection —
    // one generic hook, zero per-intent code.
    #[cfg(feature = "dev")]
    if !drained.is_empty()
        && let Some(mut recorder) =
            world.get_resource_mut::<crate::command::flight_recorder::FlightRecorder>()
    {
        for message in &drained {
            recorder.record_intent(M::type_path(), message.as_partial_reflect());
        }
    }
    drained
}

/// Records one executed command's outcome into the dev flight recorder.
#[cfg(feature = "dev")]
fn record_command(
    world: &mut World,
    name: &'static str,
    detail: String,
    outcome: Result<(), String>,
) {
    if let Some(mut recorder) =
        world.get_resource_mut::<crate::command::flight_recorder::FlightRecorder>()
    {
        recorder.record_command(name, detail, outcome);
    }
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
        commands.push(Box::new(CommitTransformCommand {
            changes: intent.changes,
        }));
    }
    for intent in drain::<SpawnJointIntent>(world) {
        commands.push(Box::new(SpawnJointCommand {
            record: intent.record,
            locked_before: None,
        }));
    }
    for intent in drain::<LoadSceneIntent>(world) {
        commands.push(Box::new(LoadSceneCommand::new(intent.scene)));
    }
    for intent in drain::<PropertyEditIntent>(world) {
        commands.push(Box::new(PropertyEditCommand {
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
    for intent in drain::<CutIntent>(world) {
        commands.push(Box::new(CutCommand::new(intent.a, intent.b, intent.width)));
    }
    for intent in drain::<DeleteJointIntent>(world) {
        commands.push(Box::new(DeleteJointCommand::new(intent.id)));
    }
    for intent in drain::<MergeIntent>(world) {
        commands.push(Box::new(MergeCommand::new(intent.targets)));
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
        execute(&mut stack, world, commands, undos, redos);
        world.insert_resource(crate::command::HistoryInfo {
            undo_depth: stack.undo_len(),
            redo_depth: stack.redo_len(),
        });
    });
}

/// Applies the batch through the stack (and, under `dev`, records each
/// executed command's outcome into the flight recorder).
fn execute(
    stack: &mut CommandStack,
    world: &mut World,
    commands: Vec<Box<dyn GameCommand>>,
    undos: usize,
    redos: usize,
) {
    for command in commands {
        #[cfg(feature = "dev")]
        let (name, detail) = (command.name(), format!("{command:?}"));
        // Errors are already logged by push_apply; a failed intent is
        // consumed without corrupting history.
        let result = stack.push_apply(command, world);
        #[cfg(feature = "dev")]
        record_command(world, name, detail, result.map_err(|e| e.to_string()));
        #[cfg(not(feature = "dev"))]
        let _ = result;
    }
    for _ in 0..undos {
        let undone = stack.undo(world);
        #[cfg(feature = "dev")]
        if let Some(name) = undone {
            record_command(
                world,
                crate::command::intent::name::UNDO,
                format!("undo {name}"),
                Ok(()),
            );
        }
        #[cfg(not(feature = "dev"))]
        let _ = undone;
    }
    for _ in 0..redos {
        let redone = stack.redo(world);
        #[cfg(feature = "dev")]
        if let Some(name) = redone {
            record_command(
                world,
                crate::command::intent::name::REDO,
                format!("redo {name}"),
                Ok(()),
            );
        }
        #[cfg(not(feature = "dev"))]
        let _ = redone;
    }
}
