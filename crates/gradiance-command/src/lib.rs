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

/// A forward world mutation.
///
/// `apply` must either fully succeed or leave the world untouched and return
/// an error; failed commands are never recorded. Reversal is not a command's
/// concern — the [`CommandStack`] snapshots authored state around each apply
/// and restores it on undo/redo — so `undo` is dead (a follow-up removes it
/// and each command's now-redundant apply-time capture). Commands reference
/// bodies by [`StableId`] and resolve entities at execution time.
pub trait GameCommand: Send + Sync + std::fmt::Debug {
    /// Applies the mutation.
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError>;
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

    /// Records a completed simulation run as **one** undo step, at a
    /// play/pause boundary.
    ///
    /// If the run moved authored state, the settled state is pushed as a
    /// snapshot. That does two things: undoing from here returns to the
    /// pre-run layout, and subsequent edits diff against the *settled* poses
    /// rather than stale pre-run ones — so undoing a later edit never yanks a
    /// resting body back to where it was drawn. Only these boundaries snapshot;
    /// bodies going to sleep never do, so a scene with many sleeping islands
    /// does not flood the history.
    pub fn push_boundary(&mut self, world: &mut World) {
        if self.states.is_empty() {
            return;
        }
        if self.push_snapshot(world, intent::name::SIMULATE) {
            debug!("simulation run recorded as one undo step");
        }
    }

    /// Records a settled scene-settings edit as **one** undo step.
    ///
    /// Settings resources are written directly by the UI (invariant 4) rather
    /// than through intents, so nothing else would snapshot them; the caller
    /// (`commit_settings_edits`) debounces so a whole slider drag collapses
    /// into a single step. Unlike [`push_boundary`](Self::push_boundary) this
    /// seeds the baseline when the stack is empty, so a settings edit made
    /// before any command still leaves later edits undoable — the very first
    /// one establishes the baseline rather than being reversible.
    pub fn push_settings_boundary(&mut self, world: &mut World) {
        if self.states.is_empty() {
            self.states.push_back(SceneRecord::capture(world));
            self.labels.push_back("");
            self.cursor = 0;
            return;
        }
        if self.push_snapshot(world, intent::name::SETTINGS) {
            debug!("settings edit recorded as one undo step");
        }
    }

    /// Captures the live world as a new tip labelled `label`, dropping any
    /// redo branch. Returns `false` (recording nothing) when the world already
    /// matches the current snapshot.
    fn push_snapshot(&mut self, world: &mut World, label: &'static str) -> bool {
        let live = SceneRecord::capture(world);
        if live.authored_eq(&self.states[self.cursor]) {
            return false;
        }
        self.states.truncate(self.cursor + 1);
        self.labels.truncate(self.cursor + 1);
        self.states.push_back(live);
        self.labels.push_back(label);
        self.cursor += 1;
        self.evict_to_cap();
        true
    }

    /// Undoes the most recent command, restoring the previous authored
    /// snapshot; returns the undone command's name, or `None` at the baseline.
    ///
    /// Pausing records the run as its own step (see
    /// [`push_boundary`](Self::push_boundary)), so undoing after a run returns
    /// to the pre-run layout. Undo *during* a live run behaves the same way:
    /// the dispatcher auto-pauses and closes the run first, so the first press
    /// reverts the run instead of chasing a world that is still moving.
    pub fn undo(&mut self, world: &mut World) -> Option<&'static str> {
        if self.cursor == 0 {
            return None;
        }
        let undone = self.labels[self.cursor];
        let (from, to) = (self.cursor, self.cursor - 1);
        // Differential restore: only what this command changed is written, so
        // bodies that have settled under simulation keep their live poses.
        self.states[to].restore_diff(&self.states[from], world);
        self.cursor = to;
        debug!(name = undone, "command undone");
        Some(undone)
    }

    /// Re-applies the most recently undone command, restoring the next
    /// snapshot; returns the redone command's name, or `None` at the tip.
    pub fn redo(&mut self, world: &mut World) -> Option<&'static str> {
        if self.cursor + 1 >= self.states.len() {
            return None;
        }
        let (from, to) = (self.cursor, self.cursor + 1);
        self.states[to].restore_diff(&self.states[from], world);
        self.cursor = to;
        let redone = self.labels[to];
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

/// Records the just-finished simulation run as one undo step (see
/// [`CommandStack::push_boundary`]). Runs when the sim pauses.
fn close_sim_run(world: &mut World) {
    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
        stack.push_boundary(world);
    });
}

/// Records a settled scene-settings edit as one undo step.
///
/// The settings panels write their resources directly — the sanctioned
/// config-seam exception to invariant 4 — so no intent and no command ever
/// carries these edits, and nothing else would snapshot them. Rather than
/// plumbing commit-on-release through the reflection-driven settings grid,
/// this commits on the frame *after* the last change: dragging a gravity
/// slider marks dirty every frame and snapshots once, when it settles. The
/// one-frame latency is invisible, and a whole gesture collapses into a
/// single undo step.
///
/// Only *scene-content* settings count (see
/// [`EnvironmentRecord::scene_content_eq`](gradiance_scene::records::EnvironmentRecord::scene_content_eq));
/// grid and snap are workstation config and never enter history.
fn commit_settings_edits(
    world: &mut World,
    mut seen: Local<Option<SceneSettings>>,
    mut dirty: Local<bool>,
) {
    let Some(previous) = seen.as_ref() else {
        // First frame: establish the reference without recording anything.
        *seen = Some(SceneSettings::capture(world));
        return;
    };
    if !previous.matches(world) {
        *seen = Some(SceneSettings::capture(world));
        *dirty = true;
        return;
    }
    if !*dirty {
        return;
    }
    *dirty = false;
    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
        stack.push_settings_boundary(world);
        world.insert_resource(HistoryInfo {
            undo_depth: stack.undo_len(),
            redo_depth: stack.redo_len(),
        });
    });
}

/// The scene-content settings resources, tracked **by value**.
///
/// Bevy's change flags are not usable here: the settings window calls
/// `set_changed()` unconditionally while a tab is open and writes its fields
/// through `bypass_change_detection`, so the flags read "changed" every frame
/// the panel is visible and stay silent for some edits entirely. Comparing
/// values is the only honest signal, and it costs four equality checks per
/// frame — nothing is cloned until something actually moves.
#[derive(Clone, PartialEq)]
struct SceneSettings {
    sim: gradiance_domain::settings::SimSettings,
    render: gradiance_domain::settings::RenderSettings,
    lighting: gradiance_domain::settings::LightingSettings,
    scenery: gradiance_domain::settings::ScenerySettings,
}

impl SceneSettings {
    fn capture(world: &World) -> Self {
        fn get<R: Resource + Clone + Default>(world: &World) -> R {
            world.get_resource::<R>().cloned().unwrap_or_default()
        }
        Self {
            sim: get(world),
            render: get(world),
            lighting: get(world),
            scenery: get(world),
        }
    }

    /// True when every tracked resource still holds the recorded value.
    fn matches(&self, world: &World) -> bool {
        fn eq<R: Resource + PartialEq + Default>(world: &World, mine: &R) -> bool {
            world
                .get_resource::<R>()
                .map_or_else(|| *mine == R::default(), |live| live == mine)
        }
        eq(world, &self.sim)
            && eq(world, &self.render)
            && eq(world, &self.lighting)
            && eq(world, &self.scenery)
    }
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
        // Settings panels bypass the intent seam by design (config-seam
        // exception), so scene-content settings get their own debounced
        // snapshot. After the dispatcher, so a command and a settings edit in
        // the same frame record in the order they happened.
        app.add_systems(Update, commit_settings_edits.after(CommandDispatchSet));
        // Pausing closes a simulation run: record it as one undo step so the
        // pre-run layout stays reachable and later edits diff against the
        // settled poses.
        app.add_systems(
            OnEnter(gradiance_core::states::GameState::Paused),
            close_sim_run,
        );
        // Dev-only observability: trace the per-frame Changed<> sync-match
        // counts (the deferred pipeline's cause→symptom gap; see
        // `docs/architecture.md`). Intents and commands trace from `dispatch`.
        #[cfg(feature = "dev")]
        diagnostics::install(app);
    }
}
