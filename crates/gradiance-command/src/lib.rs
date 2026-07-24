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
//! 2. Add a `struct MyCommand` implementing [`GameCommand`] (stage on
//!    first `apply`, replay on redo — see [`spawn`] for the pattern);
//!    its `name()` returns the shared [`intent::name`] constant.
//! 3. Add one row to the `command_intents!` table in [`dispatch`] —
//!    that single row registers the message, registers the reflected type
//!    (the scripting registry binds by reflection), and dispatches the
//!    intent into your command.
//!
//! Undo/redo, history depth, and persistence then work for free; the
//! combinatorial test in `tests/joints.rs` fuzzes arbitrary command
//! sequences to prove they compose.

pub mod array_cmd;
pub mod cut_cmd;
#[cfg(feature = "dev")]
pub mod diagnostics;
pub mod dispatch;
pub mod group_cmd;
pub mod intent;
pub mod joint_cmd;
pub mod merge_cmd;
pub mod property;
pub mod scale_cmd;
pub mod scene_cmd;
pub mod spawn;
pub mod transform_cmd;

use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::shape::ShapeError;
use gradiance_scene::SceneRecord;
use std::collections::VecDeque;

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

/// The application-wide undo/redo history — a bounded timeline of
/// authored-state snapshots.
///
/// Every committed command captures a [`SceneRecord`] of the resulting authored
/// world; undo/redo move a cursor along that timeline and restore the snapshot
/// there (authored entities only — config-seam settings are never rolled back,
/// via [`SceneRecord::apply_authored`]). This makes reversal uniform and robust:
/// a command only has to *apply* its forward edit, so there is no per-command
/// `undo()` logic to get wrong, and even a mid-play edit reverts cleanly.
/// Entities are respawned on restore (same [`StableId`], fresh `Entity`), so
/// callers must hold `StableId`, never a raw `Entity`, across an undo.
#[derive(Resource, Default)]
pub struct CommandStack {
    /// Authored-state snapshots, oldest first. `states[cursor]` is the current
    /// world; earlier entries are undo targets, later entries redo targets.
    states: VecDeque<SceneRecord>,
    /// `labels[i]` names the command that produced `states[i]`; `labels[0]` is
    /// the pre-history baseline and is unnamed.
    labels: VecDeque<&'static str>,
    /// Index of the current authored state within `states`.
    cursor: usize,
}

impl CommandStack {
    /// Maximum undo depth retained; older snapshots are evicted to bound memory
    /// (each snapshot is a full authored-scene capture).
    pub const CAP: usize = 256;

    /// Applies `command`; on success captures the resulting snapshot and drops
    /// the redo branch. On failure the command is dropped and history is
    /// unchanged.
    pub fn push_apply(
        &mut self,
        mut command: Box<dyn GameCommand>,
        world: &mut World,
    ) -> Result<(), CommandError> {
        // Baseline: capture the pre-edit world once so the first command is
        // undoable. A failed first command leaves only this invisible baseline
        // (undo_len stays 0).
        if self.states.is_empty() {
            self.states.push_back(SceneRecord::capture(world));
            self.labels.push_back("");
            self.cursor = 0;
        }
        match command.apply(world) {
            Ok(()) => {
                debug!(name = command.name(), "command applied");
                // Drop any redo branch, then record the new state.
                self.states.truncate(self.cursor + 1);
                self.labels.truncate(self.cursor + 1);
                self.states.push_back(SceneRecord::capture(world));
                self.labels.push_back(command.name());
                self.cursor += 1;
                self.evict_to_cap();
                Ok(())
            }
            Err(e) => {
                warn!(name = command.name(), error = %e, "command failed");
                Err(e)
            }
        }
    }

    /// Undoes the most recent command, restoring the previous authored
    /// snapshot; returns the undone command's name, or `None` at the baseline.
    pub fn undo(&mut self, world: &mut World) -> Option<&'static str> {
        if self.cursor == 0 {
            return None;
        }
        let undone = self.labels[self.cursor];
        self.cursor -= 1;
        self.states[self.cursor].apply_authored(world);
        debug!(name = undone, "command undone");
        Some(undone)
    }

    /// Re-applies the most recently undone command, restoring the next
    /// snapshot; returns the redone command's name, or `None` at the tip.
    pub fn redo(&mut self, world: &mut World) -> Option<&'static str> {
        if self.cursor + 1 >= self.states.len() {
            return None;
        }
        self.cursor += 1;
        self.states[self.cursor].apply_authored(world);
        let redone = self.labels[self.cursor];
        debug!(name = redone, "command redone");
        Some(redone)
    }

    /// Number of commands available to undo.
    pub fn undo_len(&self) -> usize {
        self.cursor
    }

    /// Number of commands available to redo.
    pub fn redo_len(&self) -> usize {
        // `saturating_sub` guards the pre-history state (empty `states`), where
        // `len() - 1` would underflow.
        self.states.len().saturating_sub(1 + self.cursor)
    }

    /// Evicts oldest snapshots beyond [`CAP`] undo steps, keeping the cursor on
    /// the current state.
    fn evict_to_cap(&mut self) {
        while self.cursor > Self::CAP {
            self.states.pop_front();
            self.labels.pop_front();
            self.cursor -= 1;
        }
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
        // The whole command intent surface — messages + reflected types —
        // registers from the one `command_intents!` table in `dispatch`.
        dispatch::register_command_intents(app);
        // Undo/redo are meta-intents (they drive the stack, not a command).
        app.add_message::<intent::UndoIntent>();
        app.add_message::<intent::RedoIntent>();
        app.register_type::<intent::UndoIntent>();
        app.register_type::<intent::RedoIntent>();
        // Marker components that only appear inside `Option`al record
        // fields; registered explicitly so reflection sees them.
        app.register_type::<gradiance_domain::field::FieldSource>();
        app.register_type::<gradiance_domain::tracer::Tracer>();
        app.add_systems(
            Update,
            dispatch::dispatch_intents.in_set(CommandDispatchSet),
        );
        // Dev-only observability: trace the per-frame Changed<> sync-match
        // counts (the deferred pipeline's cause→symptom gap; see
        // `docs/architecture.md`). Intents and commands trace from `dispatch`.
        #[cfg(feature = "dev")]
        diagnostics::install(app);
    }
}
