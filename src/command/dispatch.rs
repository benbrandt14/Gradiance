//! The single choke point turning intents into applied commands.
//!
//! The whole intent surface lives in one [`command_intents!`] table below:
//! each row names an intent type and how it builds its command. The table
//! generates both the message/type registration (used by `CommandPlugin`)
//! and the drain loop, so adding a command touches exactly one row here
//! (plus the intent and command types themselves).

use crate::command::array_cmd::ArrayCommand;
use crate::command::cut_cmd::CutCommand;
use crate::command::group_cmd::{GroupCommand, UngroupCommand};
use crate::command::intent::name;
use crate::command::intent::{
    ArrayIntent, CommitTransformIntent, CutIntent, DeleteIntent, DeleteJointIntent,
    DuplicateIntent, GroupIntent, LoadSceneIntent, MergeIntent, PropertyEditIntent, RedoIntent,
    ScaleIntent, SpawnBodyIntent, SpawnJointIntent, SpawnNodeIntent, UndoIntent, UngroupIntent,
};
use crate::command::joint_cmd::{DeleteJointCommand, SpawnJointCommand};
use crate::command::merge_cmd::MergeCommand;
use crate::command::property::PropertyEditCommand;
use crate::command::scale_cmd::ScaleCommand;
use crate::command::scene_cmd::LoadSceneCommand;
use crate::command::spawn::{DeleteCommand, DuplicateCommand, SpawnCommand};
use crate::command::transform_cmd::CommitTransformCommand;
use crate::command::{CommandStack, GameCommand};
use bevy::prelude::*;

fn drain<M: Message + Reflect + bevy::reflect::TypePath>(world: &mut World) -> Vec<M> {
    let drained: Vec<M> = crate::core::messages::drain(world);
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

/// The intent table: one row per intent/command pair, in dispatch order.
///
/// Generates [`register_command_intents`] (message + reflected-type
/// registration, called from `CommandPlugin`) and the dispatcher's drain
/// loop, so registration and dispatch can never fall out of sync.
macro_rules! command_intents {
    ($( $intent:ty => $build:expr ),* $(,)?) => {
        /// Registers every command intent as a message **and** a reflected
        /// type (the scripting registry binds operations by reflected type
        /// name — see `docs/script-lisp-decision.md`). `register_type` pulls
        /// in each intent's transitive field types (records, domain types,
        /// avian components), giving the read-total path a complete registry
        /// without naming those types individually.
        pub(crate) fn register_command_intents(app: &mut App) {
            $(
                app.add_message::<$intent>();
                app.register_type::<$intent>();
            )*
        }

        /// Drains every pending command intent into built commands, in
        /// table order.
        fn drain_command_intents(world: &mut World, commands: &mut Vec<Box<dyn GameCommand>>) {
            $(
                for intent in drain::<$intent>(world) {
                    let build: fn($intent) -> Box<dyn GameCommand> = $build;
                    commands.push(build(intent));
                }
            )*
        }
    };
}

command_intents! {
    SpawnBodyIntent => |i| Box::new(SpawnCommand::new(i.record, name::SPAWN_BODY)),
    SpawnNodeIntent => |i| Box::new(SpawnCommand::new(i.record, name::SPAWN_NODE)),
    CommitTransformIntent => |i| Box::new(CommitTransformCommand { changes: i.changes }),
    SpawnJointIntent => |i| Box::new(SpawnJointCommand {
        record: i.record,
        locked_before: None,
    }),
    LoadSceneIntent => |i| Box::new(LoadSceneCommand::new(i.scene)),
    PropertyEditIntent => |i| Box::new(PropertyEditCommand { changes: i.changes }),
    GroupIntent => |i| Box::new(GroupCommand::new(i.targets)),
    UngroupIntent => |i| Box::new(UngroupCommand::new(i.targets)),
    ScaleIntent => |i| Box::new(ScaleCommand::new(i.targets, i.pivot, i.frame_rot, i.factors)),
    ArrayIntent => |i| Box::new(ArrayCommand::new(i.sources, i.count, i.mode)),
    CutIntent => |i| Box::new(CutCommand::new(i.a, i.b, i.width)),
    DeleteJointIntent => |i| Box::new(DeleteJointCommand::new(i.id)),
    MergeIntent => |i| Box::new(MergeCommand::new(i.targets)),
    DuplicateIntent => |i| Box::new(DuplicateCommand::new(i.sources, i.offset)),
    DeleteIntent => |i| Box::new(DeleteCommand::new(i.targets)),
}

/// Drains all pending intents, builds commands, and applies them through
/// the [`CommandStack`]. This is the **only** code that touches the stack.
///
/// Failed commands log a warning and are not recorded; the intent is
/// simply consumed.
pub fn dispatch_intents(world: &mut World) {
    let mut commands: Vec<Box<dyn GameCommand>> = Vec::new();
    drain_command_intents(world, &mut commands);

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
            record_command(world, name::UNDO, format!("undo {name}"), Ok(()));
        }
        #[cfg(not(feature = "dev"))]
        let _ = undone;
    }
    for _ in 0..redos {
        let redone = stack.redo(world);
        #[cfg(feature = "dev")]
        if let Some(name) = redone {
            record_command(world, name::REDO, format!("redo {name}"), Ok(()));
        }
        #[cfg(not(feature = "dev"))]
        let _ = redone;
    }
}
