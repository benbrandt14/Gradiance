//! The command layer: every world mutation, undoable, through one choke point.
//!
//! Tools and UI emit *intents* ([`intent`]); the [`dispatch`] system drains
//! them, builds [`GameCommand`]s, and applies them through the
//! [`CommandStack`]. Nothing else may mutate authored components.
//!
//! # Lifecycle of one edit
//!
//! ```text
//!   drag release ─▶ SpawnBodyIntent ─▶ dispatch drains it
//!                                        │
//!                                        ▼
//!                              Box<dyn GameCommand>
//!                                        │ stack.push_apply(cmd, world)
//!                                        ▼
//!                    ┌───────────────────────────────────┐
//!                    │ apply() ── Ok ─▶ pushed to undo    │
//!                    │           └ Err ▶ dropped, no trace│
//!                    └───────────────────────────────────┘
//!   Ctrl+Z ─▶ UndoIntent ─▶ stack.undo() ─▶ cmd.undo(world) ─▶ redo stack
//! ```
//!
//! # Why a trait object per edit
//!
//! Each [`GameCommand`] captures exactly what it needs to reverse itself
//! (a spawn remembers its id; a delete captures the full records it
//! removed). Because commands resolve entities by
//! [`StableId`] at execution time — never by
//! holding a raw `Entity` — they stay valid across undo/redo cycles that
//! despawn and respawn the same logical body.
//!
//! # Adding a command (the extension recipe)
//!
//! 1. Add an intent struct in [`intent`] (`#[derive(Message, Reflect)]`),
//!    a kebab-case constant in [`intent::name`], and a `// Trace:` line
//!    naming the command and the sync systems it triggers.
//! 2. Register it in [`CommandPlugin::build`] with `add_message` **and**
//!    `register_type` (the scripting registry binds by reflection).
//! 3. Add a `struct MyCommand` implementing [`GameCommand`] (stage on
//!    first `apply`, replay on redo — see [`spawn`] for the pattern);
//!    its `name()` returns the shared [`intent::name`] constant.
//! 4. Drain the intent → push the command in [`dispatch`].
//!
//! Undo/redo, history depth, and persistence then work for free; the
//! combinatorial test in `tests/joints.rs` fuzzes arbitrary command
//! sequences to prove they compose.

pub mod array_cmd;
pub mod cut_cmd;
pub mod dispatch;
#[cfg(feature = "dev")]
pub mod flight_recorder;
pub mod group_cmd;
pub mod intent;
pub mod joint_cmd;
pub mod merge_cmd;
pub mod property;
pub mod scale_cmd;
pub mod scene_cmd;
pub mod snapshot;
pub mod spawn;
pub mod transform_cmd;

use crate::core::ids::{IdIndex, StableId};
use crate::domain::shape::ShapeError;
use bevy::prelude::*;

/// Resolves a stable id to its live entity — the standard first step of
/// every command's `apply`/`undo` (shared so the `MissingEntity` mapping
/// exists once).
pub(crate) fn resolve(world: &World, id: StableId) -> Result<Entity, CommandError> {
    world
        .resource::<IdIndex>()
        .entity(id)
        .ok_or(CommandError::MissingEntity(id))
}

/// Why a command could not be applied.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CommandError {
    /// The command's shape parameters were invalid.
    #[error(transparent)]
    Shape(#[from] ShapeError),
    /// A referenced body no longer exists.
    #[error("no live entity for stable id {0}")]
    MissingEntity(StableId),
    /// The command resolved to nothing to do (not recorded in history).
    #[error("command had no effect")]
    NoEffect,
}

/// An undoable world mutation.
///
/// `apply` must either fully succeed or leave the world untouched and
/// return an error; failed commands are never recorded. `undo` reverses a
/// previously successful `apply`. Commands reference bodies by
/// [`StableId`] and resolve entities at execution time, so they stay valid
/// across undo/redo cycles that respawn entities.
pub trait GameCommand: Send + Sync + std::fmt::Debug {
    /// Applies the mutation.
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError>;
    /// Reverses a successful [`apply`](GameCommand::apply).
    fn undo(&mut self, world: &mut World) -> Result<(), CommandError>;
    /// Short human-readable name (for logs and UI).
    fn name(&self) -> &'static str;
}

/// The application-wide undo/redo history.
#[derive(Resource, Default)]
pub struct CommandStack {
    undo: Vec<Box<dyn GameCommand>>,
    redo: Vec<Box<dyn GameCommand>>,
}

impl CommandStack {
    /// Applies `command`; on success records it and clears the redo branch.
    ///
    /// On failure the command is dropped and the history is unchanged.
    pub fn push_apply(
        &mut self,
        mut command: Box<dyn GameCommand>,
        world: &mut World,
    ) -> Result<(), CommandError> {
        match command.apply(world) {
            Ok(()) => {
                debug!(name = command.name(), "command applied");
                self.undo.push(command);
                self.redo.clear();
                Ok(())
            }
            Err(e) => {
                warn!(name = command.name(), error = %e, "command failed");
                Err(e)
            }
        }
    }

    /// Undoes the most recent command, if any; returns the undone command's
    /// name on success.
    pub fn undo(&mut self, world: &mut World) -> Option<&'static str> {
        let mut command = self.undo.pop()?;
        match command.undo(world) {
            Ok(()) => {
                debug!(name = command.name(), "command undone");
                let name = command.name();
                self.redo.push(command);
                Some(name)
            }
            Err(e) => {
                // An un-undoable command is a bug; drop it rather than
                // corrupt the history with a half-reversed entry.
                error!(name = command.name(), error = %e, "undo failed; dropping entry");
                None
            }
        }
    }

    /// Re-applies the most recently undone command, if any; returns the
    /// redone command's name on success.
    pub fn redo(&mut self, world: &mut World) -> Option<&'static str> {
        let mut command = self.redo.pop()?;
        match command.apply(world) {
            Ok(()) => {
                debug!(name = command.name(), "command redone");
                let name = command.name();
                self.undo.push(command);
                Some(name)
            }
            Err(e) => {
                error!(name = command.name(), error = %e, "redo failed; dropping entry");
                None
            }
        }
    }

    /// Number of commands available to undo.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of commands available to redo.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

/// Read-only mirror of the history depths, refreshed by the dispatcher.
///
/// Exists so UI (and the debug tab) can show undo/redo state without
/// naming the `CommandStack` (which stays private to the dispatcher).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct HistoryInfo {
    /// Commands available to undo.
    pub undo_depth: usize,
    /// Commands available to redo.
    pub redo_depth: usize,
}

/// System set containing the intent dispatcher; producers of intents must
/// schedule `.before(CommandDispatchSet)` (or rely on the default: the
/// dispatcher runs late in `Update`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandDispatchSet;

/// Registers the command stack, all intent messages, and the dispatcher.
#[derive(Default)]
pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandStack>();
        app.init_resource::<HistoryInfo>();
        app.add_message::<intent::SpawnBodyIntent>();
        app.add_message::<intent::SpawnNodeIntent>();
        app.add_message::<intent::DeleteIntent>();
        app.add_message::<intent::DuplicateIntent>();
        app.add_message::<intent::CommitTransformIntent>();
        app.add_message::<intent::ScaleIntent>();
        app.add_message::<intent::ArrayIntent>();
        app.add_message::<intent::SpawnJointIntent>();
        app.add_message::<intent::PropertyEditIntent>();
        app.add_message::<intent::GroupIntent>();
        app.add_message::<intent::UngroupIntent>();
        app.add_message::<intent::CutIntent>();
        app.add_message::<intent::DeleteJointIntent>();
        app.add_message::<intent::MergeIntent>();
        app.add_message::<intent::LoadSceneIntent>();
        app.add_message::<intent::UndoIntent>();
        app.add_message::<intent::RedoIntent>();
        // Reflection registry: the scripting layer binds operations by
        // reflected type name (see `docs/script-lisp-decision.md`). Every
        // authored intent now derives `Reflect` (spike #1 settled
        // `StableId`/`ShapeDef` opacity — see `docs/script-spike-findings.md`),
        // so the whole intent surface is registered here. `register_type`
        // pulls in each intent's transitive field types (records, domain
        // types, avian components), giving the read-total path a complete
        // registry without naming those types individually.
        app.register_type::<intent::SpawnBodyIntent>();
        app.register_type::<intent::SpawnNodeIntent>();
        app.register_type::<crate::domain::field::FieldSource>();
        app.register_type::<crate::domain::tracer::Tracer>();
        app.register_type::<intent::DeleteIntent>();
        app.register_type::<intent::DuplicateIntent>();
        app.register_type::<intent::CommitTransformIntent>();
        app.register_type::<intent::ScaleIntent>();
        app.register_type::<intent::ArrayIntent>();
        app.register_type::<intent::SpawnJointIntent>();
        app.register_type::<intent::PropertyEditIntent>();
        app.register_type::<intent::GroupIntent>();
        app.register_type::<intent::UngroupIntent>();
        app.register_type::<intent::CutIntent>();
        app.register_type::<intent::DeleteJointIntent>();
        app.register_type::<intent::MergeIntent>();
        app.register_type::<intent::LoadSceneIntent>();
        app.register_type::<intent::UndoIntent>();
        app.register_type::<intent::RedoIntent>();
        app.add_systems(
            Update,
            dispatch::dispatch_intents.in_set(CommandDispatchSet),
        );
        // Dev-only observability: the flight recorder ring buffer
        // (`docs/architecture.md` § debugging the deferred pipeline).
        #[cfg(feature = "dev")]
        flight_recorder::install(app);
    }
}
