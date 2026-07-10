//! The World-facing scripting seam (Tier-A authoring path).
//!
//! This is the **only ECS-touching part** of `src/script/`. It embeds the
//! steel engine, registers scene-verb builtins, and runs the exclusive
//! [`run_scripts`] system that dispatches script-emitted operation records
//! through the *existing* intent bus.
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
//! Because `Reflect: Send + Sync`, the op queue (`Vec<Box<dyn Reflect>>`) is
//! `Send + Sync` and a builtin may capture and push to it; the steel engine
//! itself is the cold-path VM and lives as a `NonSend` resource on the main
//! thread, so it never needs to be `Send`.

use crate::command::CommandDispatchSet;
use crate::command::intent::{CutIntent, SpawnBodyIntent};
use crate::command::snapshot::BodyRecord;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::appearance::Appearance;
use crate::domain::layers::LayerMask32;
use crate::domain::props::BodyPhysics;
use crate::domain::shape::ShapeDef;
use crate::script::reflect_bridge::steel_to_f64;
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

/// The steel engine plus the shared op queue. A `NonSend` resource: the VM is
/// the cold authoring path and lives on the main thread, so it never needs to
/// be `Send`.
pub struct ScriptEngine {
    engine: Engine,
    ops: OpQueue,
}

impl ScriptEngine {
    fn new() -> Self {
        let ops: OpQueue = Arc::new(Mutex::new(Vec::new()));
        let mut engine = Engine::new();
        register_builtins(&mut engine, &ops);
        Self { engine, ops }
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

/// Registers the scene-verb builtins on `engine`, each capturing a clone of the
/// shared op queue. A new authored verb = one builtin here + one
/// [`IntentDispatch::register`] call in [`ScriptPlugin`].
fn register_builtins(engine: &mut Engine, ops: &OpQueue) {
    // `(cut ax ay bx by width)` — sever bodies along a stroke.
    let cut_ops = Arc::clone(ops);
    engine.register_fn(
        "cut",
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
        "spawn-box",
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
        "spawn-circle",
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

/// Exclusive: runs any pending script source, then dispatches the operation
/// records it emitted through the intent bus. Ordered before
/// [`CommandDispatchSet`] so a script's edits become commands in the same
/// frame — one script run therefore collapses to one batch of undoable edits.
pub fn run_scripts(world: &mut World) {
    let sources: Vec<String> = world
        .get_resource_mut::<ScriptInputs>()
        .map(|mut inputs| std::mem::take(&mut inputs.0))
        .unwrap_or_default();

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
    // each record through its intent bus.
    let ops: Vec<Box<dyn Reflect>> = world
        .get_non_send::<ScriptEngine>()
        .and_then(|engine| engine.ops.lock().ok().map(|mut q| std::mem::take(&mut *q)))
        .unwrap_or_default();
    if ops.is_empty() {
        return;
    }
    world.resource_scope(|world, dispatch: Mut<IntentDispatch>| {
        for op in ops {
            dispatch.apply(world, op);
        }
    });
}

/// Installs the scripting seam: embeds steel, registers the scene verbs and
/// their intent dispatch, and runs the exclusive doorway system. Off by
/// default — behind the `script` feature, and confined to `src/script/`.
#[derive(Default)]
pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptInputs>();
        app.init_resource::<ScriptLog>();
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
}
