//! The World-facing scripting seam (Tier-A authoring path).
//!
//! This is the **only ECS-touching part** of `src/script/`. It embeds the
//! steel engine, registers scene-verb builtins across all three governance
//! categories (authored-world **edits** → intents, **config** → settings
//! resources, read-total **queries**), and runs the exclusive [`run_scripts`]
//! system that dispatches script-emitted operation records through the
//! *existing* seams.
//!
//! # The World-integration constraint (spike #1)
//!
//! steel's `register_fn` requires `Fn(..) + Send + Sync + 'static`, so a
//! builtin **cannot capture `&mut World`**. That is not a limitation to work
//! around — it *is* the architecture pointing at itself. A builtin only ever
//! *emits operation data* (a reflected intent value) into a queue; one
//! exclusive system drains the queue and writes each value to its
//! `Messages<T>` — exactly the doorway tools and UI already use. Scripts
//! therefore **physically cannot** bypass the command discipline (invariants
//! 1–2): they can only emit intents, and only for intents an operation was
//! registered for. The registry may `write_message`, never `get_mut` an
//! authored component.
//!
//! # Reads are total, writes are seam-mediated
//!
//! The same `Send + Sync` constraint blocks a builtin from reading `&World`
//! too. The read half of the governance model (`docs/script-lisp-decision.md`)
//! is served by a [`SceneSnapshot`]: [`run_scripts`] snapshots the committed scene
//! into a shared buffer before running a source, and the query builtins read
//! that buffer. Reads therefore observe *last-committed* state (a script's own
//! edits, being pending intents, land the same frame but after this system) —
//! which is exactly the last-frame-read discipline the driver dataflow will
//! also use.
//!
//! Because `Reflect: Send + Sync`, the op queue (`Vec<Box<dyn Reflect>>`) and
//! the `SceneSnapshot` are `Send + Sync` and a builtin may capture them; the steel
//! engine itself is the cold-path VM and lives as a `NonSend` resource on the
//! main thread, so it never needs to be `Send`.

pub use crate::actions::{ScriptAction, ScriptActions};
use crate::budget::{SCRIPT_BUDGET, run_with_budget};
use crate::reflect_bridge::{read_path, steel_to_f64, write_path};
use crate::registry::{OperationCatalog, name};
use avian2d::prelude::RigidBody;
use gradiance_command::CommandDispatchSet;
use gradiance_command::snapshot::SceneSnapshot;
use gradiance_core::ids::StableId;
use gradiance_core::script_tools::{ScriptToolFired, ScriptToolSpec, ScriptTools};
use gradiance_domain::props::BodyPhysics;
use gradiance_domain::settings::{CameraSettings, SimSettings};
use gradiance_domain::signal::{ComputedSignal, SignalExpr, SignalParam};
use gradiance_op::{
    OpContext, OpEntry, OpError, OpIdSource, OpOutput, OpRegistry, SelectionMirror,
};
use steel::rerrs::{ErrorKind, SteelErr};

/// A `defparam` / `defsignal` declaration queued by a script builtin, drained
/// in [`run_scripts`] into the `SignalParams` / `ComputedSignals` resources.
enum SignalDef {
    /// A tunable slider parameter.
    Param(SignalParam),
    /// A computed (modulator) signal.
    Computed(ComputedSignal),
}
use bevy::prelude::*;
use bevy::reflect::Reflect;
use std::any::TypeId;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use steel::rvals::SteelVal;
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;

/// A queue of script-emitted writes, in call order: each registry
/// operation's id and output, for [`gradiance_op::apply`]. Shared between the
/// generated verbs (producers) and the exclusive [`run_scripts`] system
/// (consumer).
type OpQueue = Arc<Mutex<Vec<(&'static str, OpOutput)>>>;

/// A read snapshot of the committed scene, shared between [`run_scripts`]
/// (producer) and the query builtins (consumers). The same value the run's
/// [`OpContext`] holds: one snapshot per run for both kinds of verb.
type SharedView = Arc<Mutex<Arc<SceneSnapshot>>>;

/// One pending config write (a reflect-path + scalar) emitted by a config verb;
/// applied to the settings resource after the run — the invariant-#4 seam, not
/// the command stack.
struct ConfigWrite {
    /// Which settings resource the path is into.
    target: ConfigTarget,
    path: String,
    value: f64,
}

/// The settings resources the config verbs reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigTarget {
    /// `CameraSettings` (`camera-set`) — workstation config, never undone.
    Camera,
}

/// A queue of config writes, shared between the config builtins (producers) and
/// [`run_scripts`] (consumer).
type ConfigQueue = Arc<Mutex<Vec<ConfigWrite>>>;

/// The buffers the steel builtins capture, cloned into each closure. A producer
/// / consumer split between the builtins and [`run_scripts`]: query/`sim-get`
/// verbs read the snapshots [`run_scripts`] fills; edit/`sim-set` verbs push to
/// the queues [`run_scripts`] drains.
struct Shared {
    /// Reflected edit intents emitted by edit verbs → the intent bus.
    ops: OpQueue,
    /// Read snapshot of the committed scene → query verbs.
    view: SharedView,
    /// The run pass's operation context → registry operations.
    ctx: Arc<Mutex<Arc<OpContext>>>,
    /// Read mirror of `SimSettings` → `sim-get`.
    sim: Arc<Mutex<SimSettings>>,
    /// Read mirror of `CameraSettings` → `camera-get`.
    camera: Arc<Mutex<CameraSettings>>,
    /// Config writes emitted by `sim-set` → the settings resource.
    config: ConfigQueue,
    /// Named actions emitted by `register-action` → the `ScriptActions` table.
    actions: Arc<Mutex<Vec<ScriptAction>>>,
    /// Named-signal publishes emitted by `signal-set` → the `SignalBus`.
    signal_writes: Arc<Mutex<Vec<(String, f64)>>>,
    /// Read mirror of the `SignalBus` values → `signal-get`.
    signal_values: Arc<Mutex<HashMap<String, f64>>>,
    /// Param / computed-signal declarations emitted by `defparam` /
    /// `defsignal` → the `SignalParams` / `ComputedSignals` resources.
    signal_defs: Arc<Mutex<Vec<SignalDef>>>,
    /// Workspace labels emitted by `label` → the [`WorkspaceLabels`] table
    /// (body-handle uuid string + display name).
    labels: Arc<Mutex<Vec<(String, String)>>>,
    /// Panel visibility requests emitted by `panel-show`/`-hide`/`-toggle` →
    /// the [`PanelRequests`] queue the UI drains. `None` means "flip it".
    panel_requests: Arc<Mutex<Vec<(String, Option<bool>)>>>,
    /// Read mirror of which panels are open → `panel-open?`. Published by the
    /// UI each frame, because panel state lives in `gradiance-ui` and the
    /// script layer sits below it.
    panel_states: Arc<Mutex<HashMap<String, bool>>>,
    /// The operation catalog → the `ops`/`describe` meta verbs. Refreshed
    /// when registry operations are bound.
    catalog: Arc<Mutex<OperationCatalog>>,
    /// The world-space point the current run's action was invoked at →
    /// `action-x`/`action-y`. Set by [`run_scripts`] around one run only.
    action_point: Arc<Mutex<Option<Vec2>>>,
    /// Tools emitted by `register-tool` → the [`ScriptTools`] table.
    tools: Arc<Mutex<Vec<ScriptToolSpec>>>,
    /// The points the current run's script tool collected →
    /// `tool-count`/`tool-x`/`tool-y`. Set by [`run_scripts`] around one run
    /// only.
    tool_points: ToolPoints,
}

/// The current run's script-tool points (`None` outside a tool run).
type ToolPoints = Arc<Mutex<Option<Vec<Vec2>>>>;

impl Shared {
    fn new() -> Self {
        Self {
            ops: Arc::new(Mutex::new(Vec::new())),
            view: Arc::new(Mutex::new(Arc::new(SceneSnapshot::default()))),
            ctx: Arc::new(Mutex::new(Arc::new(OpContext::new(OpIdSource::Random)))),
            sim: Arc::new(Mutex::new(SimSettings::default())),
            camera: Arc::new(Mutex::new(CameraSettings::default())),
            config: Arc::new(Mutex::new(Vec::new())),
            actions: Arc::new(Mutex::new(Vec::new())),
            signal_writes: Arc::new(Mutex::new(Vec::new())),
            signal_values: Arc::new(Mutex::new(HashMap::new())),
            signal_defs: Arc::new(Mutex::new(Vec::new())),
            labels: Arc::new(Mutex::new(Vec::new())),
            panel_requests: Arc::new(Mutex::new(Vec::new())),
            panel_states: Arc::new(Mutex::new(HashMap::new())),
            catalog: Arc::new(Mutex::new(OperationCatalog::builtin())),
            action_point: Arc::new(Mutex::new(None)),
            tools: Arc::new(Mutex::new(Vec::new())),
            tool_points: Arc::new(Mutex::new(None)),
        }
    }

    /// How far each write queue has filled — taken before a run, so a failed
    /// run can be [rolled back](Self::rollback) to exactly here without
    /// touching what earlier runs in the same frame queued.
    fn mark(&self) -> QueueMark {
        QueueMark([
            queue_len(&self.ops),
            queue_len(&self.config),
            queue_len(&self.actions),
            queue_len(&self.signal_writes),
            queue_len(&self.signal_defs),
            queue_len(&self.labels),
            queue_len(&self.panel_requests),
            queue_len(&self.tools),
        ])
    }

    /// Discards everything queued since `mark`: a run is all or nothing, like
    /// its one undo batch, so a script that fails (eval error, panic, budget)
    /// half-way commits none of its edits, config writes, labels, actions,
    /// tools, panel requests or signal publishes.
    fn rollback(&self, mark: &QueueMark) {
        let [ops, config, actions, writes, defs, labels, panels, tools] = mark.0;
        queue_truncate(&self.ops, ops);
        queue_truncate(&self.config, config);
        queue_truncate(&self.actions, actions);
        queue_truncate(&self.signal_writes, writes);
        queue_truncate(&self.signal_defs, defs);
        queue_truncate(&self.labels, labels);
        queue_truncate(&self.panel_requests, panels);
        queue_truncate(&self.tools, tools);
    }
}

/// The fill level of every [`Shared`] write queue at one instant (see
/// [`Shared::mark`]).
struct QueueMark([usize; 8]);

fn queue_len<T>(queue: &Mutex<Vec<T>>) -> usize {
    queue.lock().map_or(0, |q| q.len())
}

fn queue_truncate<T>(queue: &Mutex<Vec<T>>, len: usize) {
    if let Ok(mut q) = queue.lock() {
        q.truncate(len);
    }
}

/// Why a script could not run.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The steel engine reported an evaluation error.
    #[error("script evaluation failed: {0}")]
    Eval(String),
    /// The script panicked (caught at the eval boundary).
    #[error("script panicked")]
    Panicked,
    /// The script ran past its wall-clock budget
    /// ([`SCRIPT_BUDGET`]) and was stopped.
    #[error("stopped after {ms} ms: budget exceeded")]
    Budget {
        /// The budget the run exceeded, in milliseconds.
        ms: u128,
    },
}

/// One queued run: the source, plus the invocation context it was submitted
/// with.
#[derive(Debug, Clone)]
pub struct ScriptRun {
    /// The lisp source.
    pub source: String,
    /// The world-space point an action was invoked at (the context menu's
    /// click point); `None` for a REPL line, a file, or a test submission.
    /// Readable during this run only, via `(action-x)` / `(action-y)`.
    pub at: Option<Vec2>,
    /// The points a script tool collected (world space, click order), for a
    /// tool's run; `None` for every other run. Readable during this run only,
    /// via `(tool-count)` / `(tool-x i)` / `(tool-y i)`.
    pub points: Option<Vec<Vec2>>,
    /// Where to leave this run's outcome for a caller that waits on it (the
    /// remote control surface); `None` for every other submission, whose
    /// outcome reaches the user through the [`ScriptLog`] only.
    pub reply: Option<RunReply>,
}

/// One run's outcome as a caller outside the console sees it: the run's last
/// value as JSON (`null` for no value), or the same error text the console
/// log shows.
pub type RunOutcome = Result<serde_json::Value, String>;

/// A slot [`run_scripts`] fills with one run's [`RunOutcome`].
pub type RunReply = Arc<Mutex<Option<RunOutcome>>>;

/// Pending script runs for the next [`run_scripts`] pass (REPL input, a
/// `--script` file, an invoked action, or a test). Drained each frame.
#[derive(Resource, Default)]
pub struct ScriptInputs(Vec<ScriptRun>);

impl ScriptInputs {
    /// Queues one script source to run next frame, with no invocation
    /// context (`(action-x)` reads NaN).
    pub fn submit(&mut self, source: impl Into<String>) {
        self.0.push(ScriptRun {
            source: source.into(),
            at: None,
            points: None,
            reply: None,
        });
    }

    /// Queues one script source like [`submit`](Self::submit), and returns
    /// the slot its [`RunOutcome`] lands in once [`run_scripts`] has run it.
    pub fn submit_with_reply(&mut self, source: impl Into<String>) -> RunReply {
        let reply = RunReply::default();
        self.0.push(ScriptRun {
            source: source.into(),
            at: None,
            points: None,
            reply: Some(Arc::clone(&reply)),
        });
        reply
    }

    /// Queues an action's source to run next frame, invoked at the
    /// world-space point `at`. The point is passed *in* by the caller (the
    /// UI owns the click), never read up the layer graph by the script.
    pub fn invoke(&mut self, action: &ScriptAction, at: Vec2) {
        self.0.push(ScriptRun {
            source: action.source.clone(),
            at: Some(at),
            points: None,
            reply: None,
        });
    }

    /// Queues a script tool's body to run next frame, with the points its
    /// gesture collected. This is the tool's **one** VM entry per gesture:
    /// the per-frame work (collecting, snapping, previewing) is Rust's.
    pub fn invoke_tool(&mut self, tool: &ScriptToolSpec, points: Vec<Vec2>) {
        self.0.push(ScriptRun {
            source: tool.source.clone(),
            at: None,
            points: Some(points),
            reply: None,
        });
    }
}

/// Paths to `.scm` files to run once at startup — a user's init script, or
/// files passed on the CLI (`--script foo.scm`). Each is read and submitted to
/// [`ScriptInputs`], so it runs through the ordinary doorway on the first frame:
/// the natural place to `register-action`, build a scene, or define helpers.
/// Missing or unreadable files warn and are skipped.
#[derive(Resource, Default)]
pub struct StartupScripts(pub Vec<PathBuf>);

/// Startup system: reads each configured script file, queues its source, and
/// arms the [`ScriptWatch`] hot-reload poll for it.
fn load_startup_scripts(
    scripts: Res<StartupScripts>,
    mut inputs: ResMut<ScriptInputs>,
    mut watch: ResMut<ScriptWatch>,
) {
    for path in &scripts.0 {
        match std::fs::read_to_string(path) {
            Ok(source) => inputs.submit(source),
            Err(err) => warn!("startup script {}: {err}", path.display()),
        }
        watch.files.push((path.clone(), mtime(path)));
    }
}

/// Hot-reload watcher over the `--script` files: polls mtimes on the cold
/// authoring path and re-submits a file through the ordinary
/// [`ScriptInputs`] doorway when it changes on disk.
///
/// Re-running a file converges for *definitions* (`register-action` and
/// friends upsert by name — see [`ScriptActions`]); *edit* verbs re-apply as
/// ordinary new undoable commands. Semantics for authors: `docs/scripting.md`.
#[derive(Resource)]
pub struct ScriptWatch {
    /// Watched files, each with the mtime last seen (`None` = unreadable at
    /// the last poll, so a file that appears later triggers a load).
    files: Vec<(PathBuf, Option<std::time::SystemTime>)>,
    /// Seconds between polls. The default (0.5 s) is imperceptible for
    /// authoring; tests set `0.0` to poll every frame.
    pub interval: f32,
    /// Seconds accumulated since the last poll.
    elapsed: f32,
}

impl Default for ScriptWatch {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            interval: 0.5,
            elapsed: 0.0,
        }
    }
}

/// A file's modification time, or `None` when unreadable.
fn mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Update system: re-submits a watched script when its mtime changes.
///
/// `Time` is optional so the headless test app (which runs no time plugin)
/// can drive the watcher with `interval = 0.0`.
fn watch_scripts(
    time: Option<Res<Time>>,
    mut watch: ResMut<ScriptWatch>,
    mut inputs: ResMut<ScriptInputs>,
) {
    if watch.files.is_empty() {
        return;
    }
    watch.elapsed += time.map_or(0.0, |t| t.delta_secs());
    if watch.elapsed < watch.interval {
        return;
    }
    watch.elapsed = 0.0;
    for (path, seen) in &mut watch.files {
        let now = mtime(path);
        if now == *seen {
            continue;
        }
        *seen = now;
        match std::fs::read_to_string(&*path) {
            Ok(source) => {
                info!(path = %path.display(), "script changed on disk — reloading");
                inputs.submit(source);
            }
            Err(err) => warn!("script reload {}: {err}", path.display()),
        }
    }
}

/// The operation catalog surfaced as a resource, so the console (and later
/// data-driven menus / user tools) read what the VM understands — completion,
/// highlighting, and the reference panel all track the real op set. Built from
/// the same [`OperationCatalog::builtin`] the steel builtins register against,
/// so the advertised surface never drifts from the implemented one.
#[derive(Resource, Default)]
pub struct OperationRegistry(pub OperationCatalog);

/// Rebuilds the surfaced catalog when the operation registry changes, so the
/// console and the remote catalog list the registry's verbs too.
fn sync_catalog(registry: Res<OpRegistry>, mut catalog: ResMut<OperationRegistry>) {
    if registry.is_changed() {
        catalog.0 = OperationCatalog::with_registry(&registry);
    }
}

/// One entry in the console log: a submitted source and its outcome.
#[derive(Debug, Clone)]
pub struct ScriptEntry {
    /// The source that ran.
    pub input: String,
    /// Outcome text (`ok`, or the error message).
    pub output: String,
    /// Whether the run succeeded.
    pub ok: bool,
}

/// A rolling log of script runs and their outcomes — the console's output pane.
/// Capped so a long REPL session cannot grow unbounded.
#[derive(Resource, Default)]
pub struct ScriptLog(pub Vec<ScriptEntry>);

impl ScriptLog {
    /// Newest entries a console keeps.
    const CAP: usize = 500;

    fn record(&mut self, entry: ScriptEntry) {
        self.0.push(entry);
        if self.0.len() > Self::CAP {
            let overflow = self.0.len() - Self::CAP;
            self.0.drain(0..overflow);
        }
    }
}

/// Script-given names on scene bodies — the **workspace** (MATLAB cue):
/// `(define b (spawn-box …))` + `(label b "crate")` makes the body wear a
/// name tag in the viewport and in pick lists. `EditorState`: never
/// persisted, never undoable; the `StableId` underneath is the durable
/// identity.
#[derive(Resource, Default, Debug)]
pub struct WorkspaceLabels(pub Vec<(String, StableId)>);

impl WorkspaceLabels {
    /// Adds or replaces the binding for `name` (labels are unique by name,
    /// like variables).
    pub fn upsert(&mut self, name: String, id: StableId) {
        if let Some(slot) = self.0.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = id;
        } else {
            self.0.push((name, id));
        }
    }

    /// The label on `id`, if any.
    pub fn name_of(&self, id: StableId) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, i)| *i == id)
            .map(|(n, _)| n.as_str())
    }
}

/// Panel visibility changes a script asked for, drained by the UI.
///
/// The UI owns panel state (`gradiance-ui` is above `gradiance-script` in the
/// layer graph), so a verb cannot write it directly — nor should it, since that
/// is the same **`EditorState`** seam the View menu uses. The script queues a
/// request by name; the UI applies it through
/// `panels::PanelToggle::set_open`, exactly as a menu click does. That is what
/// makes "a menu item is a registered op" true rather than aspirational.
///
/// An unknown name is reported once and dropped — a typo should say so, not
/// fail silently and not abort the run.
#[derive(Resource, Default, Debug)]
pub struct PanelRequests(pub Vec<PanelRequest>);

/// One queued panel change: the panel's name, and what to do to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRequest {
    /// The panel's script-facing name (lower-case, e.g. `"properties"`).
    pub name: String,
    /// `Some(true)` show, `Some(false)` hide, `None` flip.
    pub shown: Option<bool>,
}

/// Which panels are currently open, published by the UI for `panel-open?`.
///
/// Reads are total, so a script may ask about any panel; the mirror is how a
/// *read* crosses the layer boundary the other way. Rebuilt each frame, never
/// persisted.
#[derive(Resource, Default, Debug)]
pub struct PanelStates(pub Vec<(String, bool)>);

/// Routes a reflected intent value to its `Messages<T>` bus. This is the
/// operation registry's dispatch half: a script names an op, the exclusive
/// system routes the reflected value to the right intent channel. Registering
/// an intent here is the *only* way a script op reaches the world, and it can
/// only ever `write_message`.
#[derive(Resource, Default)]
pub struct IntentDispatch {
    writers: HashMap<TypeId, fn(&mut World, Box<dyn Reflect>)>,
}

impl IntentDispatch {
    /// Registers `T` so a script-emitted `T` value dispatches to its bus.
    pub fn register<T: Message + Reflect>(&mut self) {
        self.writers.insert(TypeId::of::<T>(), |world, op| {
            if let Ok(intent) = op.into_any().downcast::<T>() {
                world.write_message(*intent);
            }
        });
    }

    /// Whether an intent type is registered (the registry-validation test
    /// checks every Edit op's intent against this).
    pub fn is_registered(&self, intent: TypeId) -> bool {
        self.writers.contains_key(&intent)
    }
}

/// One Edit-category operation's binding to the intent type it emits.
/// [`edit_bindings`] is the single table [`ScriptPlugin`] registers
/// [`IntentDispatch`] from and `tests/it/registry_validation.rs` checks the
/// catalog against.
pub struct EditBinding {
    /// The operation's [`name`] constant.
    pub op: &'static str,
    /// `TypeId` of the intent the verb emits.
    pub intent: TypeId,
    /// The intent's reflected type path (must resolve in the type registry).
    pub intent_path: &'static str,
    /// Registers the intent's bus writer on an [`IntentDispatch`].
    pub register: fn(&mut IntentDispatch),
}

/// Every hand-written Edit-category verb and the intent it emits — one row
/// per verb. Registry operations (`gradiance-op`) are not here: they lower
/// through their own declaration.
pub fn edit_bindings() -> Vec<EditBinding> {
    // Every edit verb is a registry operation now (U-2a); the table and its
    // type go in U-2b.
    Vec::new()
}

/// The steel engine plus the shared buffers it reads/writes. A `NonSend`
/// resource: the VM is the cold authoring path and lives on the main thread, so
/// it never needs to be `Send`.
pub struct ScriptEngine {
    engine: Engine,
    shared: Shared,
    /// How many [`OpRegistry`] entries are bound as verbs.
    bound_ops: usize,
}

impl ScriptEngine {
    fn new() -> Self {
        let shared = Shared::new();
        let mut engine = Engine::new();
        register_builtins(&mut engine, &shared);
        Self {
            engine,
            shared,
            bound_ops: 0,
        }
    }

    /// Binds each registry operation not bound yet as a Lisp verb, and
    /// refreshes the catalog the meta verbs read. Layers register operations
    /// at app build; binding at the first run catches every one of them.
    fn bind_ops(&mut self, registry: &OpRegistry) {
        let entries = registry.entries();
        if entries.len() <= self.bound_ops {
            return;
        }
        for entry in &entries[self.bound_ops..] {
            bind_op(&mut self.engine, &self.shared, entry.clone());
        }
        self.bound_ops = entries.len();
        if let Ok(mut catalog) = self.shared.catalog.lock() {
            *catalog = OperationCatalog::with_registry(registry);
        }
    }

    /// Runs one script source under the [wall-clock budget](crate::budget),
    /// converting steel errors to [`ScriptError`]. All or nothing: if the run
    /// fails, everything it queued is discarded before this returns. Wrapped
    /// in `catch_unwind` so a panicking script never takes down the app.
    /// Returns the run's last value, which is also bound to the global `ans`
    /// (MATLAB-style) so the next REPL line can reuse it.
    /// Sets (or clears) the invocation point the next run reads.
    fn set_action_point(&self, at: Option<Vec2>) {
        if let Ok(mut point) = self.shared.action_point.lock() {
            *point = at;
        }
    }

    /// Sets (or clears) the script-tool points the next run reads.
    fn set_tool_points(&self, points: Option<Vec<Vec2>>) {
        if let Ok(mut slot) = self.shared.tool_points.lock() {
            *slot = points;
        }
    }

    fn run(&mut self, source: &str) -> Result<Option<SteelVal>, ScriptError> {
        let mark = self.shared.mark();
        let result = self.run_guarded(source);
        if result.is_err() {
            self.shared.rollback(&mark);
        }
        result
    }

    fn run_guarded(&mut self, source: &str) -> Result<Option<SteelVal>, ScriptError> {
        let engine = &mut self.engine;
        // `Engine::run` wants an owned/`'static` source (`Into<Cow<'static,
        // str>>`), so hand it an owned copy.
        let owned = source.to_owned();
        let controller = engine.get_thread_state_controller();
        let (outcome, tripped) = run_with_budget(&controller, SCRIPT_BUDGET, || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.run(owned)))
        });
        let values = match outcome {
            Err(_) => return Err(ScriptError::Panicked),
            // A trip surfaces as a steel error; the watchdog says which.
            Ok(Err(_)) if tripped => {
                return Err(ScriptError::Budget {
                    ms: SCRIPT_BUDGET.as_millis(),
                });
            }
            Ok(Err(e)) => return Err(ScriptError::Eval(e.to_string())),
            Ok(Ok(values)) => values,
        };
        let last = values.into_iter().next_back();
        if let Some(value) = &last
            && !matches!(value, SteelVal::Void)
        {
            engine.register_value("ans", value.clone());
        }
        Ok(last)
    }
}

/// Coerces `N` steel values to finite `f64`s, or `None` if any is non-numeric
/// or non-finite (Scheme integer literals arrive as `IntV`, floats as `NumV` —
/// [`steel_to_f64`] accepts both). A verb with a bad argument no-ops rather
/// than emitting a malformed intent.
fn nums<const N: usize>(vals: [&SteelVal; N]) -> Option<[f64; N]> {
    let out = vals.map(|v| steel_to_f64(v).unwrap_or(f64::NAN));
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// Coerces one steel value to a non-negative array index, or `None`.
fn index_arg(v: &SteelVal) -> Option<usize> {
    steel_to_f64(v)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| n as usize)
}

/// Binds one registry operation as a Lisp verb, generated from its
/// declaration: the arguments map to the named params by declared field order
/// (the same mapping every doorway uses), the registry's one serde decoder
/// reads them, and the operation lowers against the run pass's context. The
/// output joins the run's queue, so a failed run discards it like any other
/// write. This is the whole Lisp-specific part: no decoder of its own.
fn bind_op(engine: &mut Engine, shared: &Shared, entry: OpEntry) {
    let ctx = Arc::clone(&shared.ctx);
    let ops = Arc::clone(&shared.ops);
    let lisp = entry.meta.lisp;
    let call = move |args: &[SteelVal]| -> steel::rvals::Result<SteelVal> {
        let failed = |err: OpError| SteelErr::new(ErrorKind::Generic, format!("{lisp}: {err}"));
        let params = entry
            .named(args.iter().map(steel_to_json).collect())
            .map_err(failed)?;
        let ctx = ctx.lock().map(|ctx| Arc::clone(&ctx)).map_err(|_| {
            SteelErr::new(
                ErrorKind::Generic,
                format!("{lisp}: the operation context is unavailable"),
            )
        })?;
        let output = entry.lower(params, &ctx).map_err(failed)?;
        let value = output.value().map_or(SteelVal::Void, json_to_steel);
        if let Ok(mut queue) = ops.lock() {
            queue.push((entry.meta.id, output));
        }
        Ok(value)
    };
    engine.register_value(lisp, SteelVal::anonymous_boxed_function(Arc::new(call)));
}

/// A registry operation's return value as steel data: the inverse of
/// [`steel_to_json`] for the shapes an operation returns.
fn json_to_steel(value: &serde_json::Value) -> SteelVal {
    use serde_json::Value;
    match value {
        Value::Null => SteelVal::Void,
        Value::Bool(b) => SteelVal::BoolV(*b),
        Value::Number(n) => SteelVal::NumV(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(text) => SteelVal::StringV(text.as_str().into()),
        Value::Array(items) => SteelVal::ListV(items.iter().map(json_to_steel).collect()),
        Value::Object(_) => SteelVal::StringV(value.to_string().into()),
    }
}

/// Registers the scene-verb builtins on `engine`. Edit verbs capture the op
/// queue; query verbs capture the read snapshot; config verbs capture the sim
/// mirror + config queue; meta verbs capture the catalog. Adding an authored
/// verb = one builtin here (under a [`name`] constant) + one entry in
/// [`OperationCatalog::builtin`] + (for edits) one [`IntentDispatch::register`]
/// call in [`ScriptPlugin`].
fn register_builtins(engine: &mut Engine, shared: &Shared) {
    register_property_verbs(engine, &shared.view);
    register_query_verbs(engine, &shared.view);
    register_config_verbs(engine, &shared.config, &shared.sim, &shared.camera);
    register_editor_verbs(engine, &shared.actions, &shared.labels);
    register_action_reads(engine, &shared.action_point);
    register_tool_verbs(engine, &shared.tools, &shared.tool_points);
    register_panel_verbs(engine, &shared.panel_requests, &shared.panel_states);
    register_signal_verbs(
        engine,
        &shared.signal_writes,
        &shared.signal_values,
        &shared.signal_defs,
    );
    register_meta_verbs(engine, &shared.catalog);
}

/// Read-total geometric queries — each reads the [`SceneSnapshot`] snapshot and
/// returns a number, mutating nothing.
fn register_query_verbs(engine: &mut Engine, view: &SharedView) {
    let v = Arc::clone(view);
    engine.register_fn(name::JOINT_COUNT, move || -> f64 {
        v.lock().map_or(f64::NAN, |view| view.joint_count())
    });

    let v = Arc::clone(view);
    engine.register_fn(name::BODY_X, move |i: SteelVal| -> f64 {
        match (v.lock(), index_arg(&i)) {
            (Ok(view), Some(i)) => view.x(i),
            _ => f64::NAN,
        }
    });

    let v = Arc::clone(view);
    engine.register_fn(name::BODY_Y, move |i: SteelVal| -> f64 {
        match (v.lock(), index_arg(&i)) {
            (Ok(view), Some(i)) => view.y(i),
            _ => f64::NAN,
        }
    });

    let v = Arc::clone(view);
    engine.register_fn(name::BODY_ROT, move |i: SteelVal| -> f64 {
        match (v.lock(), index_arg(&i)) {
            (Ok(view), Some(i)) => view.rot(i),
            _ => f64::NAN,
        }
    });

    let v = Arc::clone(view);
    engine.register_fn(name::COUNT_AT, move |x: SteelVal, y: SteelVal| -> f64 {
        match (nums([&x, &y]), v.lock()) {
            (Some([x, y]), Ok(view)) => view.count_at(Vec2::new(x as f32, y as f32)),
            _ => 0.0,
        }
    });

    let v = Arc::clone(view);
    engine.register_fn(name::NEAREST_AT, move |x: SteelVal, y: SteelVal| -> f64 {
        match (nums([&x, &y]), v.lock()) {
            (Some([x, y]), Ok(view)) => view.nearest_at(Vec2::new(x as f32, y as f32)),
            _ => -1.0,
        }
    });

    let v = Arc::clone(view);
    engine.register_fn(name::NEAREST_DIST, move |x: SteelVal, y: SteelVal| -> f64 {
        match (nums([&x, &y]), v.lock()) {
            (Some([x, y]), Ok(view)) => view.nearest_dist(Vec2::new(x as f32, y as f32)),
            _ => -1.0,
        }
    });

    let v = Arc::clone(view);
    engine.register_fn(
        name::BODY_INDEX_AT,
        move |x: SteelVal, y: SteelVal| -> f64 {
            match (nums([&x, &y]), v.lock()) {
                (Some([x, y]), Ok(view)) => view.index_at(Vec2::new(x as f32, y as f32)),
                _ => -1.0,
            }
        },
    );

    // `(touch-count i)` — how many bodies the i-th body is touching.
    let v = Arc::clone(view);
    engine.register_fn(name::TOUCH_COUNT, move |i: SteelVal| -> f64 {
        match (steel_to_f64(&i), v.lock()) {
            (Some(i), Ok(view)) if i >= 0.0 => view.touch_count(i as usize),
            _ => f64::NAN,
        }
    });
}

/// Editor configuration — `sim-get` reads the settings mirror (a total read),
/// `sim-set` queues a reflect-path write applied to the settings resource (the
/// invariant-#4 seam, never the command stack). Both name fields only by
/// reflect-path, so any `SimSettings` scalar is reachable with no per-field
/// builtin. `camera-get` / `camera-set` are the same pair over
/// `CameraSettings`.
fn register_config_verbs(
    engine: &mut Engine,
    config: &ConfigQueue,
    sim: &Arc<Mutex<SimSettings>>,
    camera: &Arc<Mutex<CameraSettings>>,
) {
    // `sim-set` is the registry's `sim.set`; only the read is bound here.
    register_config_get(engine, sim, name::SIM_GET);
    register_config_get(engine, camera, name::CAMERA_GET);
    register_config_set(
        engine,
        config,
        camera,
        name::CAMERA_SET,
        ConfigTarget::Camera,
    );
}

/// One `(…-get path)` over a settings mirror: read a setting by reflect-path.
fn register_config_get<T: Reflect>(engine: &mut Engine, mirror: &Arc<Mutex<T>>, get: &'static str) {
    let m = Arc::clone(mirror);
    engine.register_fn(get, move |path: String| -> f64 {
        m.lock()
            .ok()
            .and_then(|settings| read_path(&*settings, &path))
            .unwrap_or(f64::NAN)
    });
}

/// One `(…-set path value)` over a settings mirror.
fn register_config_set<T: Reflect + Clone>(
    engine: &mut Engine,
    config: &ConfigQueue,
    mirror: &Arc<Mutex<T>>,
    set: &'static str,
    target: ConfigTarget,
) {
    // `(…-set path value)` — queue a config write to a setting. The write is
    // trialled on a copy of the mirror first, so a bad path or a non-numeric
    // value is a script error (logged and shown in the console) rather than a
    // write that silently never lands. Integer and float literals both coerce
    // to the leaf's type (`0` and `0.0` are the same).
    let c = Arc::clone(config);
    let m = Arc::clone(mirror);
    engine.register_fn(
        set,
        move |path: String, value: SteelVal| -> Result<(), String> {
            let Some(number) = steel_to_f64(&value) else {
                return Err(format!("{set} {path}: expected a number, got {value}"));
            };
            if let Ok(settings) = m.lock() {
                let mut trial = settings.clone();
                write_path(&mut trial, &path, number).map_err(|e| format!("{set}: {e}"))?;
            }
            if let Ok(mut queue) = c.lock() {
                queue.push(ConfigWrite {
                    target,
                    path,
                    value: number,
                });
            }
            Ok(())
        },
    );
}

/// Editor-state verbs — writes to non-authored editor resources. `register-action`
/// appends to the action table the UI surfaces; it queues the entry (the exclusive
/// system moves it into the `ScriptActions` resource), never touching the World.
fn register_editor_verbs(
    engine: &mut Engine,
    actions: &Arc<Mutex<Vec<ScriptAction>>>,
    labels: &Arc<Mutex<Vec<(String, String)>>>,
) {
    // `(register-action label source)` — surface a named action in the editor.
    let a = Arc::clone(actions);
    engine.register_fn(
        name::REGISTER_ACTION,
        move |label: String, source: String| {
            if let Ok(mut queue) = a.lock() {
                queue.push(ScriptAction::user(label, source));
            }
        },
    );

    // `(label body name)` — give a body a visible workspace name. `body` is
    // the handle a spawn verb returned (`ans` right after spawning).
    let l = Arc::clone(labels);
    engine.register_fn(name::LABEL, move |body: String, label_name: String| {
        if let Ok(mut queue) = l.lock() {
            queue.push((body, label_name));
        }
    });
}

/// Invocation-context reads: where the running action was invoked. The UI
/// passes the click point in with the invocation ([`ScriptInputs::invoke`]);
/// a run with no invocation (a REPL line, a file) reads NaN.
fn register_action_reads(engine: &mut Engine, point: &Arc<Mutex<Option<Vec2>>>) {
    let p = Arc::clone(point);
    engine.register_fn(name::ACTION_X, move || -> f64 {
        match p.lock() {
            Ok(guard) => guard.map_or(f64::NAN, |v| f64::from(v.x)),
            Err(_) => f64::NAN,
        }
    });
    let p = Arc::clone(point);
    engine.register_fn(name::ACTION_Y, move || -> f64 {
        match p.lock() {
            Ok(guard) => guard.map_or(f64::NAN, |v| f64::from(v.y)),
            Err(_) => f64::NAN,
        }
    });
}

/// Most points one script tool may collect: a tool that asks for more is a
/// typo, not a gesture anyone will finish.
const MAX_TOOL_POINTS: usize = 64;

/// Script-tool verbs: `register-tool` (editor state — queues a palette entry
/// the exclusive system moves into [`ScriptTools`]) and the per-run reads of
/// the points a tool's gesture collected. The reads are NaN outside a tool
/// run, like `(action-x)`; inside one, an index past the last point is an
/// error, because it can only be a bug in the tool's body.
fn register_tool_verbs(
    engine: &mut Engine,
    tools: &Arc<Mutex<Vec<ScriptToolSpec>>>,
    points: &ToolPoints,
) {
    let t = Arc::clone(tools);
    engine.register_fn(
        name::REGISTER_TOOL,
        move |tool_name: String, count: SteelVal, source: String| -> Result<(), String> {
            let tool_name = tool_name.trim().to_string();
            if tool_name.is_empty() {
                return Err(format!("{}: the name is empty", name::REGISTER_TOOL));
            }
            let needed = steel_to_f64(&count)
                .filter(|n| n.fract() == 0.0 && *n >= 1.0 && *n <= MAX_TOOL_POINTS as f64)
                .ok_or_else(|| {
                    format!(
                        "{}: points must be a whole number from 1 to {MAX_TOOL_POINTS}, got {count}",
                        name::REGISTER_TOOL
                    )
                })?;
            if let Ok(mut queue) = t.lock() {
                queue.push(ScriptToolSpec {
                    name: tool_name,
                    points: needed as usize,
                    source,
                });
            }
            Ok(())
        },
    );

    let p = Arc::clone(points);
    engine.register_fn(name::TOOL_COUNT, move || -> f64 {
        p.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|pts| pts.len() as f64))
            .unwrap_or(f64::NAN)
    });
    for (verb, axis) in [(name::TOOL_X, 0_usize), (name::TOOL_Y, 1)] {
        let p = Arc::clone(points);
        engine.register_fn(verb, move |i: SteelVal| -> Result<f64, String> {
            let Ok(guard) = p.lock() else {
                return Ok(f64::NAN);
            };
            let Some(pts) = guard.as_ref() else {
                return Ok(f64::NAN);
            };
            let point = steel_to_f64(&i)
                .filter(|n| n.fract() == 0.0 && *n >= 0.0)
                .and_then(|n| pts.get(n as usize))
                .ok_or_else(|| {
                    format!("{verb}: no point {i} (the tool collected {})", pts.len())
                })?;
            Ok(f64::from(point[axis]))
        });
    }
}

/// Panel verbs — the script side of the View menu.
///
/// Three writes (`panel-show`, `panel-hide`, `panel-toggle`) queue a request
/// the UI drains; one read (`panel-open?`) consults the mirror the UI
/// publishes. No verb touches panel state directly: the layer graph forbids it,
/// and the `EditorState` seam is the point.
fn register_panel_verbs(
    engine: &mut Engine,
    requests: &Arc<Mutex<Vec<(String, Option<bool>)>>>,
    states: &Arc<Mutex<HashMap<String, bool>>>,
) {
    for (verb, shown) in [
        (name::PANEL_SHOW, Some(true)),
        (name::PANEL_HIDE, Some(false)),
        (name::PANEL_TOGGLE, None),
    ] {
        let q = Arc::clone(requests);
        engine.register_fn(verb, move |panel: String| {
            if let Ok(mut queue) = q.lock() {
                queue.push((panel, shown));
            }
        });
    }

    let m = Arc::clone(states);
    engine.register_fn(name::PANEL_OPEN, move |panel: String| -> bool {
        m.lock()
            .ok()
            .and_then(|states| states.get(panel.trim()).copied())
            .unwrap_or(false)
    });
}

/// Body-property reads. The writes (`set-friction` …) are registry operations
/// (`gradiance_command::ops::body`); the reads are deliberately symmetric:
/// `set-friction` and `body-friction` name the same quantity, so a script can
/// round-trip a value it did not author. Reads are total; writes are
/// seam-mediated.
fn register_property_verbs(engine: &mut Engine, view: &SharedView) {
    // Reads — the same quantities, so a script can inspect before it edits.
    for (verb, read) in [
        (
            name::BODY_FRICTION,
            (|p: &BodyPhysics| f64::from(p.friction.dynamic_coefficient))
                as fn(&BodyPhysics) -> f64,
        ),
        (name::BODY_RESTITUTION, |p: &BodyPhysics| {
            f64::from(p.restitution.coefficient)
        }),
        (name::BODY_DENSITY, |p: &BodyPhysics| f64::from(p.density.0)),
        (name::BODY_STATIC, |p: &BodyPhysics| {
            f64::from(u8::from(p.rigid_body == RigidBody::Static))
        }),
    ] {
        let v = Arc::clone(view);
        engine.register_fn(verb, move |i: SteelVal| -> f64 {
            match (v.lock(), index_arg(&i)) {
                (Ok(view), Some(i)) => view.physics(i).map_or(f64::NAN, read),
                _ => f64::NAN,
            }
        });
    }
}

/// Signal-dataflow verbs — the script side of the signal bus
/// (`docs/signal-dataflow.md`). `signal-set` queues a named publish (the
/// exclusive system moves it onto the `SignalBus`); `signal-get` reads the
/// per-run value mirror. Scripts thereby *feed* the dataflow on their own
/// cold runs — the per-frame evaluator never enters the VM.
fn register_signal_verbs(
    engine: &mut Engine,
    writes: &Arc<Mutex<Vec<(String, f64)>>>,
    values: &Arc<Mutex<HashMap<String, f64>>>,
    defs: &Arc<Mutex<Vec<SignalDef>>>,
) {
    // `(signal-set name value)` — publish a named value on the signal bus.
    let w = Arc::clone(writes);
    engine.register_fn(name::SIGNAL_SET, move |name: String, value: SteelVal| {
        if let (Ok(mut queue), Some(value)) = (w.lock(), steel_to_f64(&value)) {
            queue.push((name, value));
        }
    });

    // `(signal-get name)` — the current value of a bus signal (NaN if unset).
    let v = Arc::clone(values);
    engine.register_fn(name::SIGNAL_GET, move |name: String| -> f64 {
        v.lock()
            .ok()
            .and_then(|values| values.get(&name).copied())
            .unwrap_or(f64::NAN)
    });

    // `(defparam name value min max)` — declare a tunable slider param.
    let d = Arc::clone(defs);
    engine.register_fn(
        name::DEFPARAM,
        move |name: String, value: SteelVal, min: SteelVal, max: SteelVal| {
            if let (Ok(mut queue), Some(value), Some(min), Some(max)) = (
                d.lock(),
                steel_to_f64(&value),
                steel_to_f64(&min),
                steel_to_f64(&max),
            ) {
                queue.push(SignalDef::Param(SignalParam {
                    name,
                    value: value as f32,
                    min: min as f32,
                    max: max as f32,
                }));
            }
        },
    );

    // `(defsignal name expr)` — declare a computed signal from RPN source.
    let d = Arc::clone(defs);
    engine.register_fn(name::DEFSIGNAL, move |name: String, expr: String| {
        if let (Ok(mut queue), Ok(parsed)) = (d.lock(), SignalExpr::parse_rpn(&expr)) {
            queue.push(SignalDef::Computed(ComputedSignal {
                name,
                expr: parsed,
                block: None,
            }));
        }
    });
}

/// Homoiconic introspection — the catalog reading itself.
fn register_meta_verbs(engine: &mut Engine, catalog: &Arc<Mutex<OperationCatalog>>) {
    // `(ops)` — a list of every registered operation name.
    let cat = Arc::clone(catalog);
    engine.register_fn(name::OPS, move || -> SteelVal {
        cat.lock().map_or(SteelVal::Void, |cat| {
            SteelVal::ListV(cat.names().map(|n| SteelVal::StringV(n.into())).collect())
        })
    });

    // `(describe name)` — the signature and doc of one operation, as text.
    let cat = Arc::clone(catalog);
    engine.register_fn(name::DESCRIBE, move |n: String| -> String {
        cat.lock()
            .ok()
            .and_then(|cat| {
                cat.find(&n)
                    .map(|op| format!("{}  —  {}", op.signature, op.doc))
            })
            .unwrap_or_else(|| format!("unknown op: {n}"))
    });
}

/// A run's value as JSON, for callers outside the console: numbers, booleans,
/// strings and lists map to their JSON forms, `void` to `null`, and anything
/// else (a procedure, a hash map, …) to the text the console would echo.
fn steel_to_json(value: &SteelVal) -> serde_json::Value {
    use serde_json::Value;
    match value {
        SteelVal::Void => Value::Null,
        SteelVal::BoolV(b) => Value::Bool(*b),
        SteelVal::IntV(i) => Value::from(*i),
        SteelVal::NumV(n) => serde_json::Number::from_f64(*n)
            .map_or_else(|| Value::String(n.to_string()), Value::Number),
        SteelVal::StringV(s) => Value::String(s.to_string()),
        SteelVal::ListV(items) => Value::Array(items.iter().map(steel_to_json).collect()),
        SteelVal::VectorV(items) => Value::Array(items.iter().map(steel_to_json).collect()),
        other => Value::String(other.to_string()),
    }
}

/// Leaves one run's outcome in its caller's slot: the value as JSON, or the
/// console's error text.
fn fill_reply(reply: &RunReply, result: &Result<Option<SteelVal>, ScriptError>, output: &str) {
    if let Ok(mut slot) = reply.lock() {
        *slot = Some(match result {
            Ok(Some(value)) => Ok(steel_to_json(value)),
            Ok(None) => Ok(serde_json::Value::Null),
            Err(_) => Err(output.to_owned()),
        });
    }
}

/// Exclusive: refreshes the read snapshot, runs any pending script source, then
/// dispatches the operation records it emitted through the intent bus. Ordered
/// before [`CommandDispatchSet`] so a script's edits become commands in the same
/// frame — one script run therefore collapses to one batch of undoable edits.
pub fn run_scripts(world: &mut World) {
    queue_tool_runs(world);
    let sources: Vec<ScriptRun> = world
        .get_resource_mut::<ScriptInputs>()
        .map_or_default(|mut inputs| std::mem::take(&mut inputs.0));
    if sources.is_empty() {
        return;
    }

    // Bind registry operations added since the last pass.
    if let Some(registry) = world.get_resource::<OpRegistry>().cloned()
        && let Some(mut engine) = world.get_non_send_mut::<ScriptEngine>()
    {
        engine.bind_ops(&registry);
    }

    // Refresh the read-total snapshots from committed state: one operation
    // context for the whole pass (scripting rule 3), whose scene snapshot the
    // hand-written query verbs read too.
    // Scripting rule 2: a script names objects by index or id, never by what
    // happens to be selected, so its context has no selection mirror and
    // `"selection"` is a visible error.
    let mut ctx = OpContext::build(world);
    ctx.remove::<SelectionMirror>();
    let ctx = Arc::new(ctx);
    let snapshot = ctx.get_shared::<SceneSnapshot>().unwrap_or_default();
    if let Some(engine) = world.get_non_send::<ScriptEngine>() {
        if let Ok(mut guard) = engine.shared.view.lock() {
            *guard = snapshot;
        }
        if let Ok(mut guard) = engine.shared.ctx.lock() {
            *guard = ctx;
        }
    }
    if let (Some(sim), Some(settings)) = (
        world
            .get_non_send::<ScriptEngine>()
            .map(|e| Arc::clone(&e.shared.sim)),
        world.get_resource::<SimSettings>().cloned(),
    ) && let Ok(mut guard) = sim.lock()
    {
        *guard = settings;
    }
    if let (Some(camera), Some(settings)) = (
        world
            .get_non_send::<ScriptEngine>()
            .map(|e| Arc::clone(&e.shared.camera)),
        world.get_resource::<CameraSettings>().cloned(),
    ) && let Ok(mut guard) = camera.lock()
    {
        *guard = settings;
    }
    refresh_signal_mirror(world);
    refresh_panel_mirror(world);

    for run in sources {
        run_one(world, run);
    }

    // Drain the op queue (releasing the engine borrow first), then write each
    // registry operation's output in call order through `gradiance_op::apply`.
    let ops: Vec<(&'static str, OpOutput)> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .ops
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    for (id, output) in ops {
        gradiance_op::apply(world, id, output);
    }

    // Drain the config queue and apply each write to the settings resource (the
    // invariant-#4 seam). Only touch the resource when a write is pending, so a
    // script that never calls `sim-set` leaves change detection alone.
    let writes: Vec<ConfigWrite> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .config
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    for write in writes {
        // Already trialled by the verb; a failure here means the resource
        // and the mirror disagree on shape — say so. Each write touches only
        // its own resource, so a script that never calls `camera-set` leaves
        // `CameraSettings` change detection alone (and likewise for sim).
        let result = match write.target {
            ConfigTarget::Camera => world
                .get_resource_mut::<CameraSettings>()
                .map(|mut s| write_path(&mut *s, &write.path, write.value)),
        };
        if let Some(Err(err)) = result {
            warn!("{:?} config write: {err}", write.target);
        }
    }

    drain_labels(world);
    drain_actions(world);
    drain_tools(world);
    drain_panel_requests(world);
    drain_signal_publishes(world);
    drain_signal_defs(world);
}

/// Drains the label queue into the `WorkspaceLabels` table (upsert by
/// name, so re-running a labelling script is idempotent).
fn drain_labels(world: &mut World) {
    let labels: Vec<(String, String)> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .labels
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    if !labels.is_empty()
        && let Some(mut table) = world.get_resource_mut::<WorkspaceLabels>()
    {
        for (handle, label_name) in labels {
            if let Ok(uuid) = uuid::Uuid::parse_str(handle.trim()) {
                table.upsert(label_name, StableId(uuid));
            } else {
                warn!("(label …): `{handle}` is not a body handle");
            }
        }
    }
}

/// Moves queued panel requests onto the [`PanelRequests`] resource for the UI
/// to apply. Appends rather than replaces: two scripts in one frame both get
/// their say, and the UI clears the queue when it drains it.
fn drain_panel_requests(world: &mut World) {
    let requests: Vec<(String, Option<bool>)> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .panel_requests
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    if !requests.is_empty()
        && let Some(mut queue) = world.get_resource_mut::<PanelRequests>()
    {
        queue
            .0
            .extend(requests.into_iter().map(|(name, shown)| PanelRequest {
                name: name.trim().to_lowercase(),
                shown,
            }));
    }
}

/// Mirrors [`PanelStates`] into the `panel-open?` read snapshot (part of the
/// per-run read refresh).
fn refresh_panel_mirror(world: &mut World) {
    if let (Some(mirror), Some(states)) = (
        world
            .get_non_send::<ScriptEngine>()
            .map(|engine| Arc::clone(&engine.shared.panel_states)),
        world.get_resource::<PanelStates>(),
    ) && let Ok(mut mirror) = mirror.lock()
    {
        mirror.clear();
        for (name, open) in &states.0 {
            mirror.insert(name.clone(), *open);
        }
    }
}

/// Drains the action queue into the `ScriptActions` table (upsert by
/// label, so re-running a registration script is idempotent).
fn drain_actions(world: &mut World) {
    let actions: Vec<ScriptAction> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .actions
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    if !actions.is_empty()
        && let Some(mut table) = world.get_resource_mut::<ScriptActions>()
    {
        for action in actions {
            table.upsert(action);
        }
    }
}

/// Runs one queued source with its invocation context (click point, tool
/// points) visible to it alone, then records the outcome (console log, and
/// the caller's reply slot if it waits on one).
fn run_one(world: &mut World, run: ScriptRun) {
    let ScriptRun {
        source,
        at,
        points,
        reply,
    } = run;
    // NonSend: the VM lives on the main thread (this exclusive system is
    // already there). Errors are surfaced (log + console), never fatal.
    // The invocation point is visible to this run only.
    let result = match world.get_non_send_mut::<ScriptEngine>() {
        Some(mut engine) => {
            engine.set_action_point(at);
            engine.set_tool_points(points);
            let result = engine.run(&source);
            engine.set_action_point(None);
            engine.set_tool_points(None);
            result
        }
        None => return,
    };
    let output = match &result {
        // MATLAB-style echo: the run's last value (bound to `ans`).
        Ok(Some(value)) if !matches!(value, SteelVal::Void) => value.to_string(),
        Ok(_) => "ok".to_string(),
        Err(err) => {
            warn!("{err}");
            err.to_string()
        }
    };
    if let Some(reply) = reply {
        fill_reply(&reply, &result, &output);
    }
    if let Some(mut log) = world.get_resource_mut::<ScriptLog>() {
        log.record(ScriptEntry {
            input: source,
            output,
            ok: result.is_ok(),
        });
    }
}

/// Turns each completed script-tool gesture ([`ScriptToolFired`], sent by the
/// interaction layer's tool driver) into one queued run of that tool's body,
/// with the gesture's points as the run's context. A gesture for an index the
/// table does not hold is dropped with a warning.
fn queue_tool_runs(world: &mut World) {
    let fired = gradiance_core::messages::drain::<ScriptToolFired>(world);
    if fired.is_empty() {
        return;
    }
    let runs: Vec<(ScriptToolSpec, Vec<Vec2>)> = {
        let Some(table) = world.get_resource::<ScriptTools>() else {
            return;
        };
        fired
            .into_iter()
            .filter_map(|f| {
                let spec = table.get(f.tool);
                if spec.is_none() {
                    warn!("script tool {} is not registered", f.tool);
                }
                spec.map(|spec| (spec.clone(), f.points))
            })
            .collect()
    };
    if let Some(mut inputs) = world.get_resource_mut::<ScriptInputs>() {
        for (spec, points) in runs {
            inputs.invoke_tool(&spec, points);
        }
    }
}

/// Drains the `register-tool` queue into the [`ScriptTools`] table (upsert
/// by name, so a hot reload re-registers in place and indices stay valid).
fn drain_tools(world: &mut World) {
    let tools: Vec<ScriptToolSpec> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .tools
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    if !tools.is_empty()
        && let Some(mut table) = world.get_resource_mut::<ScriptTools>()
    {
        for tool in tools {
            let index = table.upsert(tool);
            if u16::try_from(index).is_err() {
                warn!("script tool {index} is past the palette's index range");
            }
        }
    }
}

/// Mirrors the current [`SignalBus`](gradiance_signal::SignalBus) values into
/// the `signal-get` read snapshot (part of the per-run read refresh).
fn refresh_signal_mirror(world: &mut World) {
    if let (Some(values), Some(bus)) = (
        world
            .get_non_send::<ScriptEngine>()
            .map(|e| Arc::clone(&e.shared.signal_values)),
        world.get_resource::<gradiance_signal::SignalBus>(),
    ) && let Ok(mut guard) = values.lock()
    {
        guard.clear();
        guard.extend(
            bus.entries()
                .map(|(name, entry)| (name.to_owned(), f64::from(entry.value()))),
        );
    }
}

/// Drains `signal-set` publishes onto the signal bus (remembering the
/// names, so bus hygiene keeps script signals alive between runs).
fn drain_signal_publishes(world: &mut World) {
    let publishes: Vec<(String, f64)> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .signal_writes
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    if publishes.is_empty() {
        return;
    }
    if let Some(mut names) = world.get_resource_mut::<gradiance_signal::ScriptSignals>() {
        for (name, _) in &publishes {
            if !names.0.contains(name) {
                names.0.push(name.clone());
            }
        }
    }
    if let Some(mut bus) = world.get_resource_mut::<gradiance_signal::SignalBus>() {
        for (name, value) in publishes {
            bus.publish(&name, value as f32, false);
        }
    }
}

/// Drains `defparam` / `defsignal` declarations into the `SignalParams` /
/// `ComputedSignals` config resources (upsert by name, so a re-run of a
/// definitions script updates in place instead of duplicating).
fn drain_signal_defs(world: &mut World) {
    let defs: Vec<SignalDef> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| {
            engine
                .shared
                .signal_defs
                .lock()
                .ok()
                .map(|mut q| std::mem::take(&mut *q))
        })
        .unwrap_or_default();
    if defs.is_empty() {
        return;
    }
    for def in defs {
        match def {
            SignalDef::Param(param) => {
                if let Some(mut params) = world.get_resource_mut::<gradiance_signal::SignalParams>()
                {
                    match params.0.iter_mut().find(|p| p.name == param.name) {
                        Some(existing) => *existing = param,
                        None => params.0.push(param),
                    }
                }
            }
            SignalDef::Computed(signal) => {
                if let Some(mut computed) =
                    world.get_resource_mut::<gradiance_signal::ComputedSignals>()
                {
                    match computed.0.iter_mut().find(|c| c.name == signal.name) {
                        Some(existing) => *existing = signal,
                        None => computed.0.push(signal),
                    }
                }
            }
        }
    }
}

/// Installs the scripting seam: embeds steel, registers the scene verbs and
/// their intent dispatch, surfaces the operation catalog, and runs the
/// exclusive doorway system. Always on (steel is a first-class dependency);
/// confined to `src/script/`.
#[derive(Default)]
pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        // The registry operations the verbs are generated from.
        gradiance_command::ops::install(app);
        app.init_resource::<ScriptInputs>();
        app.init_resource::<ScriptLog>();
        app.init_resource::<OperationRegistry>();
        app.init_resource::<ScriptActions>();
        app.init_resource::<ScriptTools>();
        app.add_message::<ScriptToolFired>();
        app.init_resource::<PanelRequests>();
        app.init_resource::<PanelStates>();
        app.init_resource::<WorkspaceLabels>();
        app.init_resource::<StartupScripts>();
        app.init_resource::<ScriptWatch>();
        let mut dispatch = IntentDispatch::default();
        for binding in edit_bindings() {
            (binding.register)(&mut dispatch);
        }
        app.insert_resource(dispatch);
        app.insert_non_send(ScriptEngine::new());
        app.add_systems(Startup, load_startup_scripts);
        app.add_systems(
            Update,
            (
                sync_catalog,
                watch_scripts,
                run_scripts.before(CommandDispatchSet),
            )
                .chain(),
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::registry::OpCategory;
    use gradiance_command::intent::{CutIntent, SpawnBodyIntent};
    use gradiance_domain::shape::ShapeDef;

    #[derive(Resource, Default)]
    struct Cuts(Vec<CutIntent>);
    #[derive(Resource, Default)]
    struct Spawns(Vec<SpawnBodyIntent>);

    fn capture_cuts(mut reader: MessageReader<CutIntent>, mut out: ResMut<Cuts>) {
        for m in reader.read() {
            out.0.push(m.clone());
        }
    }
    fn capture_spawns(mut reader: MessageReader<SpawnBodyIntent>, mut out: ResMut<Spawns>) {
        for m in reader.read() {
            out.0.push(m.clone());
        }
    }

    /// A minimal app with the scripting seam + bus capture, but **no**
    /// `CommandPlugin` — so a test observes exactly what the doorway writes to
    /// the intent bus, with nothing draining it.
    fn bus_app() -> App {
        let mut app = App::new();
        app.add_message::<CutIntent>();
        app.add_message::<SpawnBodyIntent>();
        app.init_resource::<Cuts>();
        app.init_resource::<Spawns>();
        app.add_plugins(ScriptPlugin);
        app.add_systems(Update, (capture_cuts, capture_spawns).after(run_scripts));
        app
    }

    fn run(app: &mut App, source: &str) {
        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit(source);
        app.update();
    }

    #[test]
    fn cut_reaches_the_bus_with_integer_literals() {
        let mut app = bus_app();
        run(&mut app, "(cut 0 0 10 0 4)"); // Scheme ints arrive as IntV
        let cuts = &app.world().resource::<Cuts>().0;
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].a, Vec2::new(0.0, 0.0));
        assert_eq!(cuts[0].b, Vec2::new(10.0, 0.0));
        assert_eq!(cuts[0].width, 4.0);
    }

    #[test]
    fn float_literals_also_coerce() {
        let mut app = bus_app();
        run(&mut app, "(cut 0.0 0.0 10.5 0.0 4.0)");
        assert_eq!(app.world().resource::<Cuts>().0[0].b.x, 10.5);
    }

    #[test]
    fn spawn_verbs_reach_the_bus() {
        let mut app = bus_app();
        run(
            &mut app,
            "(begin (spawn-box 1 2 40 20) (spawn-circle 5 6 15))",
        );
        let spawns = &app.world().resource::<Spawns>().0;
        assert_eq!(spawns.len(), 2);
        assert!(matches!(
            spawns[0].record.shape,
            ShapeDef::Box { width, height } if width == 40.0 && height == 20.0
        ));
        assert_eq!(spawns[0].record.pose.pos, Vec2::new(1.0, 2.0));
        assert!(matches!(
            spawns[1].record.shape,
            ShapeDef::Circle { radius } if radius == 15.0
        ));
    }

    #[test]
    fn spawn_ground_emits_a_static_half_plane() {
        let mut app = bus_app();
        run(&mut app, "(spawn-ground 0 -100 0)");
        let spawns = &app.world().resource::<Spawns>().0;
        assert_eq!(spawns.len(), 1);
        assert!(matches!(spawns[0].record.shape, ShapeDef::HalfPlane));
        assert_eq!(spawns[0].record.physics, BodyPhysics::fixed());
        assert_eq!(spawns[0].record.pose.pos, Vec2::new(0.0, -100.0));
    }

    #[test]
    fn several_ops_in_one_run_preserve_order() {
        let mut app = bus_app();
        run(
            &mut app,
            "(begin (cut 0 0 1 0 1) (cut 0 0 2 0 1) (cut 0 0 3 0 1))",
        );
        let cuts = &app.world().resource::<Cuts>().0;
        assert_eq!(cuts.len(), 3);
        assert_eq!(cuts[0].b.x, 1.0);
        assert_eq!(cuts[2].b.x, 3.0);
    }

    #[test]
    fn a_non_numeric_arg_is_an_error_and_emits_nothing() {
        let mut app = bus_app();
        run(&mut app, "(cut \"oops\" 0 0 0 1)");
        assert!(app.world().resource::<Cuts>().0.is_empty());
    }

    #[test]
    fn a_script_error_is_non_fatal_and_emits_nothing() {
        let mut app = bus_app();
        // Unbound symbol → steel eval error; the frame must still complete.
        run(&mut app, "(this-is-not-a-builtin 1 2)");
        assert!(app.world().resource::<Cuts>().0.is_empty());
        assert!(app.world().resource::<Spawns>().0.is_empty());
    }

    #[test]
    fn config_verbs_write_the_settings_resource() {
        let mut app = bus_app();
        app.insert_resource(SimSettings::default());
        run(
            &mut app,
            "(begin (sim-set \"gravity.y\" -250) (sim-set \"speed\" 2))",
        );
        let settings = app.world().resource::<SimSettings>();
        assert_eq!(settings.gravity.value().y, -250.0);
        assert_eq!(settings.speed, 2.0);
        // A config write is not an authored edit — nothing reached the intent bus.
        assert!(app.world().resource::<Spawns>().0.is_empty());
    }

    #[test]
    fn camera_verbs_read_and_write_camera_settings() {
        let mut app = bus_app();
        app.insert_resource(SimSettings::default());
        app.insert_resource(CameraSettings::default());
        run(
            &mut app,
            "(camera-set \"zoom_step\" (- (camera-get \"zoom_step\") 0.04))",
        );
        let camera = app.world().resource::<CameraSettings>();
        assert!((camera.zoom_step - 0.90).abs() < 1e-6, "{camera:?}");
        // Only the camera moved: the sim settings saw no write.
        assert_eq!(
            *app.world().resource::<SimSettings>(),
            SimSettings::default()
        );

        run(&mut app, "(camera-set \"no_such_field\" 1)");
        let log = app.world().resource::<ScriptLog>();
        let entry = log.0.last().expect("logged");
        assert!(
            !entry.ok && entry.output.contains("camera-set"),
            "{}",
            entry.output
        );
    }

    #[test]
    fn a_failed_sim_set_is_a_visible_script_error() {
        let mut app = bus_app();
        app.insert_resource(SimSettings::default());
        for bad in [
            "(sim-set \"gravity.z\" 0)",    // no such field
            "(sim-set \"gravity\" 0)",      // not a scalar leaf
            "(sim-set \"speed\" \"fast\")", // not a number
        ] {
            run(&mut app, bad);
            let log = &app.world().resource::<ScriptLog>().0;
            let entry = log.last().expect("the run is logged");
            assert!(
                !entry.ok,
                "{bad} should fail visibly, got {:?}",
                entry.output
            );
            assert!(entry.output.contains("sim-set"), "{}", entry.output);
        }
        assert_eq!(
            *app.world().resource::<SimSettings>(),
            SimSettings::default()
        );
    }

    #[test]
    fn sim_get_reads_the_settings_mirror() {
        let mut app = bus_app();
        app.insert_resource(SimSettings::default()); // gravity.y = -1000
        // `sim-get` reads the mirror refreshed before the run; here it drives a
        // conditional edit so the read is observable.
        run(
            &mut app,
            "(when (< (sim-get \"gravity.y\") 0) (spawn-box 0 0 10 10))",
        );
        assert_eq!(app.world().resource::<Spawns>().0.len(), 1);
    }

    #[test]
    fn register_action_populates_the_action_table() {
        let mut app = bus_app();
        run(
            &mut app,
            "(register-action \"Fill\" \"(spawn-box 0 0 10 10)\")",
        );
        let table: Vec<_> = app.world().resource::<ScriptActions>().user().collect();
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].label, "Fill");
        assert_eq!(table[0].source, "(spawn-box 0 0 10 10)");
    }

    #[test]
    fn register_tool_publishes_the_tool_table_and_upserts_by_name() {
        let mut app = bus_app();
        run(
            &mut app,
            "(register-tool \"ramp\" 2 \"(spawn-box 0 0 1 1)\")",
        );
        run(
            &mut app,
            "(register-tool \"peg\" 1 \"(spawn-circle 0 0 1)\")",
        );
        run(&mut app, "(register-tool \"ramp\" 3 \"(undo)\")");
        let table = &app.world().resource::<ScriptTools>().0;
        assert_eq!(table.len(), 2);
        assert_eq!((table[0].name.as_str(), table[0].points), ("ramp", 3));
        assert_eq!(table[0].source, "(undo)");
    }

    #[test]
    fn a_fired_tool_runs_its_body_once_with_its_points() {
        let mut app = bus_app();
        run(
            &mut app,
            "(register-tool \"dot\" 2 \"(spawn-box-rot (tool-x 1) (tool-y 1) (tool-count) 1 0.5)\")",
        );
        app.world_mut().write_message(ScriptToolFired {
            tool: 0,
            points: vec![Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0)],
        });
        app.update();
        let spawns = &app.world().resource::<Spawns>().0;
        assert_eq!(spawns.len(), 1);
        let record = &spawns[0].record;
        assert_eq!(record.pose.pos, Vec2::new(3.0, 4.0));
        assert_eq!(record.pose.rot, 0.5);
        assert_eq!(
            record.shape,
            ShapeDef::Box {
                width: 2.0,
                height: 1.0
            }
        );
    }

    #[test]
    fn a_tool_point_past_the_last_is_an_error() {
        let mut app = bus_app();
        run(&mut app, "(register-tool \"t\" 1 \"(tool-x 1)\")");
        app.world_mut().write_message(ScriptToolFired {
            tool: 0,
            points: vec![Vec2::ZERO],
        });
        app.update();
        let last = app.world().resource::<ScriptLog>().0.last().cloned();
        let last = last.expect("the tool ran");
        assert!(
            !last.ok && last.output.contains("no point 1"),
            "{}",
            last.output
        );
    }

    #[test]
    fn re_registering_a_label_updates_in_place() {
        let mut app = bus_app();
        run(&mut app, "(register-action \"A\" \"(spawn-box 0 0 1 1)\")");
        run(&mut app, "(register-action \"A\" \"(spawn-circle 0 0 1)\")");
        let table: Vec<_> = app.world().resource::<ScriptActions>().user().collect();
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].source, "(spawn-circle 0 0 1)");
    }

    #[test]
    fn a_startup_script_file_runs_on_boot() {
        let path = std::env::temp_dir().join(format!("gradiance_boot_{}.scm", std::process::id()));
        std::fs::write(&path, "(spawn-box 0 0 10 10)").expect("write temp script");
        let mut app = bus_app();
        app.insert_resource(StartupScripts(vec![path.clone()]));
        app.update(); // Startup loads + submits; Update runs it
        let spawned = app.world().resource::<Spawns>().0.len();
        let _ = std::fs::remove_file(&path);
        assert_eq!(spawned, 1);
    }

    #[test]
    fn a_startup_script_registers_actions_on_boot() {
        // The user scenario: a `.scm` loaded at startup populates the menu.
        let path =
            std::env::temp_dir().join(format!("gradiance_bootact_{}.scm", std::process::id()));
        std::fs::write(&path, "(register-action \"Boot\" \"(spawn-box 0 0 5 5)\")")
            .expect("write temp script");
        let mut app = bus_app();
        app.insert_resource(StartupScripts(vec![path.clone()]));
        app.update();
        let registered = app
            .world()
            .resource::<ScriptActions>()
            .0
            .iter()
            .any(|a| a.label == "Boot");
        let _ = std::fs::remove_file(&path);
        assert!(registered);
    }

    #[test]
    fn an_edited_startup_script_hot_reloads() {
        let path = std::env::temp_dir().join(format!("gradiance_watch_{}.scm", std::process::id()));
        std::fs::write(&path, "(register-action \"W\" \"(spawn-box 0 0 1 1)\")")
            .expect("write temp script");
        let mut app = bus_app();
        app.insert_resource(StartupScripts(vec![path.clone()]));
        app.world_mut().resource_mut::<ScriptWatch>().interval = 0.0; // poll every frame
        app.update(); // boot: load + run

        // Rewrite, forcing an mtime the poll cannot miss (same-second writes
        // would otherwise tie on filesystems with coarse timestamps).
        std::fs::write(&path, "(register-action \"W\" \"(spawn-circle 0 0 2)\")")
            .expect("rewrite temp script");
        let file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("reopen temp script");
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
            .expect("bump mtime");
        app.update(); // watch re-submits; run_scripts applies the same frame

        let table: Vec<_> = app.world().resource::<ScriptActions>().user().collect();
        let _ = std::fs::remove_file(&path);
        assert_eq!(table.len(), 1, "reload must upsert, not duplicate");
        assert_eq!(table[0].source, "(spawn-circle 0 0 2)");
    }

    #[test]
    fn a_missing_startup_script_is_skipped_not_fatal() {
        let mut app = bus_app();
        app.insert_resource(StartupScripts(vec![PathBuf::from(
            "/no/such/gradiance.scm",
        )]));
        app.update(); // must not panic
        assert!(app.world().resource::<Spawns>().0.is_empty());
    }

    #[test]
    fn the_registry_resource_lists_every_catalog_op() {
        // The console reads this; it must equal the catalog the builtins bind
        // against, registry operations included (after the first update).
        let mut app = bus_app();
        app.update();
        let registry = app.world().resource::<OperationRegistry>();
        assert!(registry.0.find("spawn-box").is_some());
        assert_eq!(
            registry.0.find("body-count").map(|o| o.category),
            Some(OpCategory::Query),
        );
        assert_eq!(
            registry.0.find("cut").map(|o| o.category),
            Some(OpCategory::Edit),
        );
    }
}
