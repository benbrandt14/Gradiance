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
use crate::command::intent::CutIntent;
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

/// Registers the scene-verb builtins on `engine`, each capturing a clone of the
/// shared op queue. A new authored verb = one builtin here + one
/// [`IntentDispatch::register`] call in [`ScriptPlugin`].
fn register_builtins(engine: &mut Engine, ops: &OpQueue) {
    // `(cut ax ay bx by width)` — sever bodies along a stroke. Args are
    // coerced from any numeric steel value (Scheme integer literals arrive as
    // `IntV`, not `NumV`).
    let cut_ops = Arc::clone(ops);
    engine.register_fn(
        "cut",
        move |ax: SteelVal, ay: SteelVal, bx: SteelVal, by: SteelVal, width: SteelVal| {
            let c = [&ax, &ay, &bx, &by, &width].map(|v| steel_to_f64(v).unwrap_or(f64::NAN));
            if c.iter().all(|v| v.is_finite())
                && let Ok(mut queue) = cut_ops.lock()
            {
                queue.push(Box::new(CutIntent {
                    a: Vec2::new(c[0] as f32, c[1] as f32),
                    b: Vec2::new(c[2] as f32, c[3] as f32),
                    width: c[4] as f32,
                }));
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
        // already there). Errors are surfaced, never fatal.
        let outcome = world
            .get_non_send_mut::<ScriptEngine>()
            .map(|mut engine| engine.run(&source));
        if let Some(Err(err)) = outcome {
            warn!("{err}");
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
        let mut dispatch = IntentDispatch::default();
        dispatch.register::<CutIntent>();
        app.insert_resource(dispatch);
        app.insert_non_send(ScriptEngine::new());
        app.add_systems(Update, run_scripts.before(CommandDispatchSet));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::float_cmp)]
    use super::*;

    /// Captures every `CutIntent` that reached the bus this frame.
    #[derive(Resource, Default)]
    struct Captured(Vec<CutIntent>);

    fn capture(mut reader: MessageReader<CutIntent>, mut out: ResMut<Captured>) {
        for intent in reader.read() {
            out.0.push(intent.clone());
        }
    }

    #[test]
    fn script_op_reaches_the_intent_bus() {
        let mut app = App::new();
        app.add_message::<CutIntent>();
        app.init_resource::<Captured>();
        app.add_plugins(ScriptPlugin);
        // Read the bus right after the doorway writes to it (no CommandPlugin,
        // so nothing else drains it — we assert the doorway in isolation).
        app.add_systems(Update, capture.after(run_scripts));

        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit("(cut 0 0 10 0 4)");
        app.update();

        let captured = &app.world().resource::<Captured>().0;
        assert_eq!(captured.len(), 1, "one CutIntent should reach the bus");
        assert_eq!(captured[0].a, Vec2::new(0.0, 0.0));
        assert_eq!(captured[0].b, Vec2::new(10.0, 0.0));
        assert_eq!(captured[0].width, 4.0);
    }

    #[test]
    fn a_script_error_is_non_fatal_and_emits_nothing() {
        let mut app = App::new();
        app.add_message::<CutIntent>();
        app.init_resource::<Captured>();
        app.add_plugins(ScriptPlugin);
        app.add_systems(Update, capture.after(run_scripts));

        // Unbound symbol → steel eval error; the frame must still complete.
        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit("(this-is-not-a-builtin 1 2)");
        app.update();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }
}
