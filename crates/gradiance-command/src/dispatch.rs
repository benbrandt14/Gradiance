//! The single choke point turning intents into applied commands.
//!
//! The whole intent surface lives in one `command_intents!` table below:
//! each row names an intent type and how it builds its command. The table
//! generates both the message/type registration (used by `CommandPlugin`)
//! and the drain loop, so adding a command touches exactly one row here
//! (plus the intent and command types themselves).

use crate::array_cmd::ArrayCommand;
use crate::cut_cmd::CutCommand;
use crate::group_cmd::{GroupCommand, UngroupCommand};
use crate::intent::name;
use crate::intent::{
    ArrayIntent, CommitTransformIntent, CutIntent, DeleteIntent, DeleteJointIntent,
    DuplicateIntent, GroupIntent, LoadSceneIntent, MergeIntent, PropertyEditIntent, RedoIntent,
    ScaleIntent, SpawnBodyIntent, SpawnJointIntent, SpawnNodeIntent, UndoIntent, UngroupIntent,
};
use crate::joint_cmd::{DeleteJointCommand, SpawnJointCommand};
use crate::merge_cmd::MergeCommand;
use crate::property::PropertyEditCommand;
use crate::scale_cmd::ScaleCommand;
use crate::scene_cmd::LoadSceneCommand;
use crate::spawn::{DeleteCommand, DuplicateCommand, SpawnCommand};
use crate::transform_cmd::CommitTransformCommand;
use crate::{CommandStack, GameCommand};
use bevy::prelude::*;
use gradiance_core::states::GameState;

fn drain<M: Message + Reflect + bevy::reflect::TypePath>(world: &mut World) -> Vec<M> {
    let drained: Vec<M> = gradiance_core::messages::drain(world);
    // Structured trace of the drained intent surface — the deferred pipeline's
    // cause end. Always compiled, `RUST_LOG`-controlled; one generic hook,
    // zero per-intent code.
    if !drained.is_empty() {
        trace!(
            target: "gradiance_command::intent",
            intent = M::type_path(),
            count = drained.len() as u64,
            "drained"
        );
    }
    drained
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

    // Navigating history during a live run implies editing, not running, so
    // undo/redo auto-pauses. Undo additionally closes the run as one step
    // first, which is what makes the *first* press revert the run rather than
    // chase a world that is still moving under it. Redo must not push a
    // boundary: that would truncate the very redo branch it is about to walk
    // into.
    let live_run = (undos > 0 || redos > 0)
        && *world.resource::<State<GameState>>().get() == GameState::Playing;
    if live_run {
        world
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
    }

    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
        if live_run && undos > 0 {
            stack.push_boundary(world);
        }
        execute(&mut stack, world, commands, undos, redos);
        world.insert_resource(crate::HistoryInfo {
            undo_depth: stack.undo_len(),
            redo_depth: stack.redo_len(),
            undo_label: stack.peek_undo(),
            redo_label: stack.peek_redo(),
        });
    });
}

/// Applies the batch through the stack. `CommandStack` logs each applied /
/// undone / redone command; explicit undo/redo additionally trace their
/// meta-op (`RUST_LOG=gradiance_command=trace`).
fn execute(
    stack: &mut CommandStack,
    world: &mut World,
    commands: Vec<Box<dyn GameCommand>>,
    undos: usize,
    redos: usize,
) {
    for command in commands {
        // push_apply logs the applied/failed command; a failed intent is
        // consumed without corrupting history.
        let _ = stack.push_apply(command, world);
    }
    for _ in 0..undos {
        if let Some(name) = stack.undo(world) {
            trace!(target: "gradiance_command::command", op = name::UNDO, command = name, "reverted");
        }
    }
    for _ in 0..redos {
        if let Some(name) = stack.redo(world) {
            trace!(target: "gradiance_command::command", op = name::REDO, command = name, "reapplied");
        }
    }
}
