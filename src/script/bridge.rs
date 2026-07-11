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
//! is served by a [`SceneView`]: [`run_scripts`] snapshots the committed scene
//! into a shared buffer before running a source, and the query builtins read
//! that buffer. Reads therefore observe *last-committed* state (a script's own
//! edits, being pending intents, land the same frame but after this system) —
//! which is exactly the last-frame-read discipline the driver dataflow will
//! also use.
//!
//! Because `Reflect: Send + Sync`, the op queue (`Vec<Box<dyn Reflect>>`) and
//! the `SceneView` are `Send + Sync` and a builtin may capture them; the steel
//! engine itself is the cold-path VM and lives as a `NonSend` resource on the
//! main thread, so it never needs to be `Send`.

use crate::command::CommandDispatchSet;
use crate::command::intent::{CutIntent, SpawnBodyIntent};
use crate::command::snapshot::BodyRecord;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::layers::LayerMask32;
use crate::domain::props::BodyPhysics;
use crate::domain::settings::SimSettings;
use crate::domain::shape::ShapeDef;
use crate::geometry::sdf;
use crate::script::reflect_bridge::{read_path, steel_to_f64, write_path};
use crate::script::registry::{OperationCatalog, name};
use bevy::prelude::*;
use bevy::reflect::Reflect;
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use steel::rvals::SteelVal;
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;

/// A queue of script-emitted operation records (reflected intent values),
/// shared between the steel builtins (producers) and the exclusive
/// [`run_scripts`] system (consumer).
type OpQueue = Arc<Mutex<Vec<Box<dyn Reflect>>>>;

/// A read snapshot of the committed scene, shared between [`run_scripts`]
/// (producer) and the query builtins (consumers).
type SharedView = Arc<Mutex<SceneView>>;

/// One pending config write (a reflect-path + scalar) emitted by a config verb;
/// applied to the settings resource after the run — the invariant-#4 seam, not
/// the command stack.
struct ConfigWrite {
    path: String,
    value: f64,
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
    /// Read mirror of `SimSettings` → `sim-get`.
    sim: Arc<Mutex<SimSettings>>,
    /// Config writes emitted by `sim-set` → the settings resource.
    config: ConfigQueue,
    /// The operation catalog → the `ops`/`describe` meta verbs.
    catalog: Arc<OperationCatalog>,
}

impl Shared {
    fn new() -> Self {
        Self {
            ops: Arc::new(Mutex::new(Vec::new())),
            view: Arc::new(Mutex::new(SceneView::default())),
            sim: Arc::new(Mutex::new(SimSettings::default())),
            config: Arc::new(Mutex::new(Vec::new())),
            catalog: Arc::new(OperationCatalog::builtin()),
        }
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
}

/// Pending script source to run on the next [`run_scripts`] pass (REPL input,
/// a `--script` file, or a test). Drained each frame.
#[derive(Resource, Default)]
pub struct ScriptInputs(pub Vec<String>);

impl ScriptInputs {
    /// Queues one script source to run next frame.
    pub fn submit(&mut self, source: impl Into<String>) {
        self.0.push(source.into());
    }
}

/// The operation catalog surfaced as a resource, so the console (and later
/// data-driven menus / user tools) read what the VM understands — completion,
/// highlighting, and the reference panel all track the real op set. Built from
/// the same [`OperationCatalog::builtin`] the steel builtins register against,
/// so the advertised surface never drifts from the implemented one.
#[derive(Resource, Default)]
pub struct OperationRegistry(pub OperationCatalog);

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

/// One body in a [`SceneView`] snapshot (world pose + authored shape).
struct BodyView {
    pos: Vec2,
    rot: f32,
    shape: ShapeDef,
}

impl BodyView {
    /// Whether the world point `p` lies inside this body's shape (exact SDF
    /// containment, transforming the point into the body's local frame).
    fn contains(&self, p: Vec2) -> bool {
        let local = Vec2::from_angle(-self.rot).rotate(p - self.pos);
        sdf::eval(&self.shape, local) <= 0.0
    }
}

/// A read-only snapshot of the committed scene, refreshed once per
/// [`run_scripts`] pass. Query builtins read from this — they cannot hold
/// `&World` — giving scripts the read-total half of the governance model with
/// no mutation path. Bodies are ordered by id so an index is stable across a
/// run.
#[derive(Default)]
pub struct SceneView {
    bodies: Vec<BodyView>,
}

impl SceneView {
    /// Number of bodies.
    fn count(&self) -> f64 {
        self.bodies.len() as f64
    }

    /// X of the `i`-th body, or NaN if out of range.
    fn x(&self, i: usize) -> f64 {
        self.bodies.get(i).map_or(f64::NAN, |b| f64::from(b.pos.x))
    }

    /// Y of the `i`-th body, or NaN if out of range.
    fn y(&self, i: usize) -> f64 {
        self.bodies.get(i).map_or(f64::NAN, |b| f64::from(b.pos.y))
    }

    /// Rotation (radians) of the `i`-th body, or NaN if out of range.
    fn rot(&self, i: usize) -> f64 {
        self.bodies.get(i).map_or(f64::NAN, |b| f64::from(b.rot))
    }

    /// How many bodies contain the world point `p`.
    fn count_at(&self, p: Vec2) -> f64 {
        self.bodies.iter().filter(|b| b.contains(p)).count() as f64
    }

    /// Index of the body whose centre is nearest `p`, or -1 if the scene is
    /// empty.
    fn nearest_at(&self, p: Vec2) -> f64 {
        self.bodies
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.pos
                    .distance_squared(p)
                    .total_cmp(&b.pos.distance_squared(p))
            })
            .map_or(-1.0, |(i, _)| i as f64)
    }
}

/// Snapshots the committed scene (bodies ordered by id) for the read builtins.
fn snapshot_scene(world: &mut World) -> SceneView {
    let mut rows: Vec<(StableId, Vec2, f32, ShapeDef)> = world
        .query_filtered::<(&StableId, &Transform, &ShapeDef), With<Body>>()
        .iter(world)
        .map(|(id, transform, shape)| {
            let pose = PosRot::from_transform(transform);
            (*id, pose.pos, pose.rot, shape.clone())
        })
        .collect();
    rows.sort_by_key(|(id, ..)| id.0);
    SceneView {
        bodies: rows
            .into_iter()
            .map(|(_, pos, rot, shape)| BodyView { pos, rot, shape })
            .collect(),
    }
}

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

    /// Dispatches one operation record to its intent bus (dropped if the type
    /// was never registered).
    fn apply(&self, world: &mut World, op: Box<dyn Reflect>) {
        if let Some(writer) = self.writers.get(&op.as_any().type_id()) {
            writer(world, op);
        }
    }
}

/// The steel engine plus the shared buffers it reads/writes. A `NonSend`
/// resource: the VM is the cold authoring path and lives on the main thread, so
/// it never needs to be `Send`.
pub struct ScriptEngine {
    engine: Engine,
    shared: Shared,
}

impl ScriptEngine {
    fn new() -> Self {
        let shared = Shared::new();
        let mut engine = Engine::new();
        register_builtins(&mut engine, &shared);
        Self { engine, shared }
    }

    /// Runs one script source, converting steel errors to [`ScriptError`].
    /// Wrapped in `catch_unwind` so a panicking script never takes down the app.
    fn run(&mut self, source: &str) -> Result<(), ScriptError> {
        let engine = &mut self.engine;
        // `Engine::run` wants an owned/`'static` source (`Into<Cow<'static,
        // str>>`), so hand it an owned copy.
        let owned = source.to_owned();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.run(owned)))
            .map_err(|_| ScriptError::Panicked)?
            .map(|_| ())
            .map_err(|e| ScriptError::Eval(e.to_string()))
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

/// Pushes one reflected operation record onto the shared queue.
fn emit(ops: &OpQueue, op: Box<dyn Reflect>) {
    if let Ok(mut queue) = ops.lock() {
        queue.push(op);
    }
}

/// A default-styled body of `shape` centered at `(x, y)` — the record a
/// `spawn-*` verb emits.
fn body_record(shape: ShapeDef, x: f64, y: f64) -> BodyRecord {
    BodyRecord {
        id: StableId::new(),
        pose: PosRot {
            pos: Vec2::new(x as f32, y as f32),
            rot: 0.0,
        },
        shape,
        physics: BodyPhysics::default(),
        appearance: Appearance::default(),
        layers: LayerMask32::default(),
        groups: Vec::new(),
        group: None,
    }
}

/// Registers the scene-verb builtins on `engine`. Edit verbs capture the op
/// queue; query verbs capture the read snapshot; config verbs capture the sim
/// mirror + config queue; meta verbs capture the catalog. Adding an authored
/// verb = one builtin here (under a [`name`] constant) + one entry in
/// [`OperationCatalog::builtin`] + (for edits) one [`IntentDispatch::register`]
/// call in [`ScriptPlugin`].
fn register_builtins(engine: &mut Engine, shared: &Shared) {
    register_edit_verbs(engine, &shared.ops);
    register_query_verbs(engine, &shared.view);
    register_config_verbs(engine, &shared.config, &shared.sim);
    register_meta_verbs(engine, &shared.catalog);
}

/// Authored-world edits — each emits a reflected intent onto the op queue.
fn register_edit_verbs(engine: &mut Engine, ops: &OpQueue) {
    // `(cut ax ay bx by width)` — sever bodies along a stroke.
    let cut_ops = Arc::clone(ops);
    engine.register_fn(
        name::CUT,
        move |ax: SteelVal, ay: SteelVal, bx: SteelVal, by: SteelVal, width: SteelVal| {
            if let Some([ax, ay, bx, by, width]) = nums([&ax, &ay, &bx, &by, &width]) {
                emit(
                    &cut_ops,
                    Box::new(CutIntent {
                        a: Vec2::new(ax as f32, ay as f32),
                        b: Vec2::new(bx as f32, by as f32),
                        width: width as f32,
                    }),
                );
            }
        },
    );

    // `(spawn-box x y w h)` — author a box body centered at (x, y).
    let box_ops = Arc::clone(ops);
    engine.register_fn(
        name::SPAWN_BOX,
        move |x: SteelVal, y: SteelVal, w: SteelVal, h: SteelVal| {
            if let Some([x, y, w, h]) = nums([&x, &y, &w, &h]) {
                let shape = ShapeDef::Box {
                    width: w as f32,
                    height: h as f32,
                };
                emit(
                    &box_ops,
                    Box::new(SpawnBodyIntent {
                        record: body_record(shape, x, y),
                    }),
                );
            }
        },
    );

    // `(spawn-circle x y r)` — author a circle body centered at (x, y).
    let circle_ops = Arc::clone(ops);
    engine.register_fn(
        name::SPAWN_CIRCLE,
        move |x: SteelVal, y: SteelVal, r: SteelVal| {
            if let Some([x, y, r]) = nums([&x, &y, &r]) {
                let shape = ShapeDef::Circle { radius: r as f32 };
                emit(
                    &circle_ops,
                    Box::new(SpawnBodyIntent {
                        record: body_record(shape, x, y),
                    }),
                );
            }
        },
    );
}

/// Read-total geometric queries — each reads the [`SceneView`] snapshot and
/// returns a number, mutating nothing.
fn register_query_verbs(engine: &mut Engine, view: &SharedView) {
    let v = Arc::clone(view);
    engine.register_fn(name::BODY_COUNT, move || -> f64 {
        v.lock().map_or(f64::NAN, |view| view.count())
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
}

/// Editor configuration — `sim-get` reads the settings mirror (a total read),
/// `sim-set` queues a reflect-path write applied to the settings resource (the
/// invariant-#4 seam, never the command stack). Both name fields only by
/// reflect-path, so any `SimSettings` scalar is reachable with no per-field
/// builtin.
fn register_config_verbs(engine: &mut Engine, config: &ConfigQueue, sim: &Arc<Mutex<SimSettings>>) {
    // `(sim-get path)` — read a simulation setting by reflect-path.
    let s = Arc::clone(sim);
    engine.register_fn(name::SIM_GET, move |path: String| -> f64 {
        s.lock()
            .ok()
            .and_then(|settings| read_path(&*settings, &path))
            .unwrap_or(f64::NAN)
    });

    // `(sim-set path value)` — queue a config write to a simulation setting.
    let c = Arc::clone(config);
    engine.register_fn(name::SIM_SET, move |path: String, value: SteelVal| {
        if let (Ok(mut queue), Some(value)) = (c.lock(), steel_to_f64(&value)) {
            queue.push(ConfigWrite { path, value });
        }
    });
}

/// Homoiconic introspection — the catalog reading itself.
fn register_meta_verbs(engine: &mut Engine, catalog: &Arc<OperationCatalog>) {
    // `(ops)` — a list of every registered operation name.
    let cat = Arc::clone(catalog);
    engine.register_fn(name::OPS, move || -> SteelVal {
        SteelVal::ListV(cat.names().map(|n| SteelVal::StringV(n.into())).collect())
    });

    // `(describe name)` — the signature and doc of one operation, as text.
    let cat = Arc::clone(catalog);
    engine.register_fn(name::DESCRIBE, move |n: String| -> String {
        cat.find(&n).map_or_else(
            || format!("unknown op: {n}"),
            |op| format!("{}  —  {}", op.signature, op.doc),
        )
    });
}

/// Exclusive: refreshes the read snapshot, runs any pending script source, then
/// dispatches the operation records it emitted through the intent bus. Ordered
/// before [`CommandDispatchSet`] so a script's edits become commands in the same
/// frame — one script run therefore collapses to one batch of undoable edits.
pub fn run_scripts(world: &mut World) {
    let sources: Vec<String> = world
        .get_resource_mut::<ScriptInputs>()
        .map(|mut inputs| std::mem::take(&mut inputs.0))
        .unwrap_or_default();
    if sources.is_empty() {
        return;
    }

    // Refresh the read-total snapshots (scene + settings mirror) from committed
    // state, releasing each engine borrow before touching the world.
    if let Some(view) = world
        .get_non_send::<ScriptEngine>()
        .map(|e| Arc::clone(&e.shared.view))
    {
        let snapshot = snapshot_scene(world);
        if let Ok(mut guard) = view.lock() {
            *guard = snapshot;
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

    for source in sources {
        // NonSend: the VM lives on the main thread (this exclusive system is
        // already there). Errors are surfaced (log + console), never fatal.
        let result = match world.get_non_send_mut::<ScriptEngine>() {
            Some(mut engine) => engine.run(&source),
            None => continue,
        };
        let output = match &result {
            Ok(()) => "ok".to_string(),
            Err(err) => {
                warn!("{err}");
                err.to_string()
            }
        };
        if let Some(mut log) = world.get_resource_mut::<ScriptLog>() {
            log.record(ScriptEntry {
                input: source,
                output,
                ok: result.is_ok(),
            });
        }
    }

    // Drain the op queue (releasing the engine borrow first), then dispatch
    // each edit record through its intent bus.
    let ops: Vec<Box<dyn Reflect>> = world
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
    if !ops.is_empty() {
        world.resource_scope(|world, dispatch: Mut<IntentDispatch>| {
            for op in ops {
                dispatch.apply(world, op);
            }
        });
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
    if !writes.is_empty()
        && let Some(mut settings) = world.get_resource_mut::<SimSettings>()
    {
        for write in writes {
            let _ = write_path(&mut *settings, &write.path, write.value);
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
        app.init_resource::<ScriptInputs>();
        app.init_resource::<ScriptLog>();
        app.init_resource::<OperationRegistry>();
        let mut dispatch = IntentDispatch::default();
        dispatch.register::<CutIntent>();
        dispatch.register::<SpawnBodyIntent>();
        app.insert_resource(dispatch);
        app.insert_non_send(ScriptEngine::new());
        app.add_systems(Update, run_scripts.before(CommandDispatchSet));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::script::registry::OpCategory;

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
    fn a_non_numeric_arg_no_ops() {
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
        assert_eq!(settings.gravity.y, -250.0);
        assert_eq!(settings.speed, 2.0);
        // A config write is not an authored edit — nothing reached the intent bus.
        assert!(app.world().resource::<Spawns>().0.is_empty());
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
    fn the_registry_resource_lists_every_catalog_op() {
        // The console reads this; it must equal the catalog the builtins bind
        // against.
        let app = bus_app();
        let registry = app.world().resource::<OperationRegistry>();
        assert!(registry.0.find(name::SPAWN_BOX).is_some());
        assert!(registry.0.find(name::BODY_COUNT).is_some());
        assert_eq!(
            registry.0.find(name::CUT).map(|o| o.category),
            Some(OpCategory::Edit),
        );
    }
}
