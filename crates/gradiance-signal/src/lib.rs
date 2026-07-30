//! Signal dataflow: scene attributes driving visual effects.
//!
//! The scaffolding for the future time-series node editor
//! (`docs/signal-dataflow.md`): a [`SignalBinding`] is the degenerate
//! two-node graph — one **source** (a read of scene state: speed, distance,
//! contact force/count, or a script-published value) wired through a
//! domain **map** and a color **gradient** into one **sink** (body fill,
//! tracer color, or the plot). The enums here become node kinds when the
//! editor grows a canvas; the [`SignalBus`] is already the wire protocol.
//!
//! Governance (the usual asymmetry):
//! - **Sources are reads** — `Transform`, the `physics::queries` facade,
//!   or the bus itself. Reads are total.
//! - **Sinks are derived writes only** — a [`SignalColorOverride`]
//!   component the render sync prefers over authored [`Appearance`]
//!   (never the authored component itself, never the command stack), or a
//!   bus/plot publish. Removing a binding removes its override and the
//!   authored appearance shows through again.
//! - [`SignalBindings`] is a **config-seam resource** (invariant-#4 class,
//!   like `GridSettings`): the UI edits it directly, it persists with the
//!   scene, it is not undoable, and it references bodies only by
//!   [`StableId`].
//! - The per-frame evaluator is plain queries + arithmetic — the scripting
//!   VM stays off the hot path. Scripts participate by *publishing* named
//!   values on their own (cold) runs via `signal-set`.
//!
//! [`Appearance`]: gradiance_domain::appearance::Appearance

pub mod compile;

use avian2d::prelude::Physics;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_core::states::GameState;
use gradiance_domain::Body;
use gradiance_physics::queries::PhysicsQueries;
use std::collections::VecDeque;

pub use gradiance_domain::signal::{
    BlockOp, ComputedSignal, ComputedSignals, Curve, CurveInterp, GradientSpec, SignalBinding,
    SignalBindings, SignalExpr, SignalMap, SignalParam, SignalParams, SignalSink, SignalSource,
    remap_binding,
};

/// The reserved input name for elapsed simulated seconds (the "clock" every
/// oscillator reads). Frozen while paused (it rides the physics clock).
pub const TIME_INPUT: &str = "t";

/// Samples of history kept per signal (~10 s at 60 fps), matching the
/// plot panel's window.
const HISTORY_CAP: usize = 600;

/// One live signal on the bus: its current value and rolling history.
///
/// [`history`](Self::history) and [`times`](Self::times) are parallel: sample
/// `i` was recorded at `times[i]` simulated seconds. Keeping the stamps is what
/// lets the plotter draw a **time** axis rather than a sample index — the two
/// differ whenever the frame rate wobbles or recording pauses mid-run (pausing
/// leaves a gap in the stamps, which is the honest picture).
#[derive(Debug, Default)]
pub struct BusEntry {
    value: f32,
    history: VecDeque<f32>,
    times: VecDeque<f32>,
}

impl BusEntry {
    /// The most recent value.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// The rolling history (oldest first).
    pub fn history(&self) -> &VecDeque<f32> {
        &self.history
    }

    /// The simulated timestamp of each recorded sample, parallel to
    /// [`history`](Self::history).
    pub fn times(&self) -> &VecDeque<f32> {
        &self.times
    }
}

/// The live signal bus: named current values + rolling histories. Derived
/// state — rebuilt continuously, never persisted. Scripts publish into it
/// with `signal-set` and read it with `signal-get`; the evaluator publishes
/// every binding; the plot panel draws the histories.
#[derive(Resource, Debug, Default)]
pub struct SignalBus {
    entries: Vec<(String, BusEntry)>,
    now: f32,
}

impl SignalBus {
    /// Stamps subsequent [`publish`](Self::publish) calls with `seconds` of
    /// simulated time. Called once at the top of each frame's signal pass, so
    /// every sample recorded that frame shares one timestamp — cheaper and
    /// more truthful than each publisher reading the clock itself.
    pub fn set_time(&mut self, seconds: f32) {
        self.now = seconds;
    }

    /// Sets `name`'s current value, appending to its history when
    /// `record` is true (recording pauses with the simulation).
    pub fn publish(&mut self, name: &str, value: f32, record: bool) {
        let index = self
            .entries
            .iter()
            .position(|(n, _)| n == name)
            .unwrap_or_else(|| {
                self.entries.push((name.to_owned(), BusEntry::default()));
                self.entries.len() - 1
            });
        let Some((_, entry)) = self.entries.get_mut(index) else {
            return;
        };
        entry.value = value;
        if record {
            entry.history.push_back(value);
            entry.times.push_back(self.now);
            while entry.history.len() > HISTORY_CAP {
                entry.history.pop_front();
                entry.times.pop_front();
            }
        }
    }

    /// The current value of `name`, if published.
    pub fn get(&self, name: &str) -> Option<f32> {
        self.entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, e)| e.value)
    }

    /// Every signal, in first-published order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &BusEntry)> {
        self.entries.iter().map(|(n, e)| (n.as_str(), e))
    }

    /// Drops signals not in `keep` (bindings that were removed), leaving
    /// script-published names alone is the caller's concern — the
    /// evaluator passes the bound names plus everything script-set.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.entries.retain(|(n, _)| keep(n));
    }
}

/// Derived per-body color override written by the evaluator and preferred
/// by the render sync over authored `Appearance`. Never serialized, never
/// in undo records; removed when its binding goes away.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct SignalColorOverride {
    /// Fill tint (`None` = authored fill shows through).
    pub fill: Option<Color>,
    /// Tracer-trail tint.
    pub trail: Option<Color>,
}

/// Names published by scripts via `signal-set` (kept alive on the bus even
/// with no binding reading them). Derived bookkeeping.
#[derive(Resource, Debug, Default)]
pub struct ScriptSignals(pub Vec<String>);

/// The compiled kernels behind [`ComputedSignals`], rebuilt by
/// [`recompile_signals`] whenever the authored set changes. Derived
/// (never persisted): a `(name → compiled)` cache plus the names that
/// failed to compile, surfaced to the UI. Keeps the scripting/kernel
/// **compile** step off the per-frame path — only [`Kernel::eval`] runs
/// each frame (`docs/signal-dataflow.md`).
///
/// [`Kernel::eval`]: gradiance_kernel::Kernel::eval
#[derive(Resource, Debug, Default)]
pub struct CompiledSignals {
    /// One compiled tape per computed signal, in authored order.
    compiled: Vec<(String, compile::CompiledSignal)>,
    /// Names whose expression failed to compile (with the reason).
    pub errors: Vec<(String, String)>,
}

/// Recompiles [`ComputedSignals`] into [`CompiledSignals`] on change (and
/// once at startup). Cold path: compilation is the VM/allocation step the
/// two-tier rule keeps out of the frame loop.
pub fn recompile_signals(computed: Res<ComputedSignals>, mut cache: ResMut<CompiledSignals>) {
    cache.compiled.clear();
    cache.errors.clear();
    for signal in &computed.0 {
        match compile::compile(&signal.expr) {
            Ok(compiled) => cache.compiled.push((signal.name.clone(), compiled)),
            Err(err) => cache.errors.push((signal.name.clone(), err.to_string())),
        }
    }
}

/// Publishes each **body-sensor** bus name a computed signal references
/// (`SignalSource::bus_name`), so the modulation tier can read a body's speed,
/// height, etc. Runs before [`evaluate_computed`] so blocks see this frame's
/// value. Targeted: only names actually referenced by a computed signal are
/// published (a disconnected sensor costs nothing).
pub fn publish_sensor_refs(
    computed: Res<ComputedSignals>,
    index: Res<IdIndex>,
    physics: PhysicsQueries,
    fixed: Res<Time<Fixed>>,
    sim: Res<Time<Physics>>,
    game: Res<State<GameState>>,
    transforms: Query<&Transform, With<Body>>,
    mut bus: ResMut<SignalBus>,
) {
    // First publisher of the frame, so this is where the frame's recording
    // timestamp is set — every sample recorded below and downstream shares it.
    bus.set_time(sim.elapsed_secs());
    let recording = *game.get() == GameState::Playing;
    let dt = fixed.timestep().as_secs_f32().max(1e-6);
    for signal in &computed.0 {
        for input in signal.expr.inputs() {
            if let Some(source) = SignalSource::from_bus_name(&input)
                && let Some(value) = read_source(&source, &index, &physics, &transforms, dt)
            {
                bus.publish(&input, value, recording);
            }
        }
    }
}

/// Publishes every parameter's value, then evaluates each computed signal
/// through its compiled kernel — the modulator layer — before the bindings
/// read the bus. Params first, so a computed signal reading a param sees
/// this frame's value; computed signals in authored order, so a chain
/// resolves within one frame (a cycle reads last frame's value, the usual
/// dataflow rule).
pub fn evaluate_computed(
    params: Res<SignalParams>,
    computed: Res<CompiledSignals>,
    time: Res<Time<Physics>>,
    game: Res<State<GameState>>,
    mut bus: ResMut<SignalBus>,
) {
    let recording = *game.get() == GameState::Playing;
    for param in &params.0 {
        bus.publish(&param.name, param.value, recording);
    }
    bus.publish(TIME_INPUT, time.elapsed_secs(), recording);
    for (name, signal) in &computed.compiled {
        let vars: Vec<f32> = signal
            .inputs
            .iter()
            .map(|input| bus.get(input).unwrap_or(0.0))
            .collect();
        let value = signal.kernel.eval(&vars);
        bus.publish(name, value, recording);
    }
}

/// Reads a scene [`SignalSource`] to a scalar — the single shared reader the
/// binding evaluator and the inspector's sensor-port readouts both use (one
/// reader, no parallel implementations). [`SignalSource::Named`] is bus-only
/// (resolved by the caller against the [`SignalBus`]); everything else is a
/// scene read through the `physics::queries` facade + `Transform`. `None` when
/// a referenced body isn't resolvable this frame.
pub fn read_source(
    source: &SignalSource,
    index: &IdIndex,
    physics: &PhysicsQueries,
    transforms: &Query<&Transform, With<Body>>,
    dt: f32,
) -> Option<f32> {
    let pos_of = |id: StableId| -> Option<Vec2> {
        index
            .entity(id)
            .and_then(|e| transforms.get(e).ok())
            .map(|t| t.translation.truncate())
    };
    match source {
        SignalSource::Speed(id) => index
            .entity(*id)
            .and_then(|e| physics.velocity_of(e))
            .map(|(v, _)| v.magnitude().value()),
        SignalSource::Spin(id) => index
            .entity(*id)
            .and_then(|e| physics.velocity_of(e))
            .map(|(_, w)| w.value().abs()),
        SignalSource::Height(id) => pos_of(*id).map(|p| p.y),
        SignalSource::PosX(id) => pos_of(*id).map(|p| p.x),
        SignalSource::Distance(a, b) => match (pos_of(*a), pos_of(*b)) {
            (Some(pa), Some(pb)) => Some(pa.distance(pb)),
            _ => None,
        },
        SignalSource::ContactForce(id) => index
            .entity(*id)
            .map(|e| physics.net_contact_impulse(e).magnitude().value() / dt),
        SignalSource::ContactCount(id) => {
            index.entity(*id).map(|e| physics.touching_count(e) as f32)
        }
        SignalSource::KineticEnergy(id) => index
            .entity(*id)
            .and_then(|e| Some(physics.kinetic_energy_of(e)?.value())),
        SignalSource::Momentum(id) => index
            .entity(*id)
            .and_then(|e| Some(physics.momentum_of(e)?.value())),
        SignalSource::AngularMomentum(id) => index
            .entity(*id)
            .and_then(|e| Some(physics.angular_momentum_of(e)?.value())),
        SignalSource::Named(_) => None,
    }
}

/// Evaluates every binding: reads its source, publishes to the bus, and
/// writes/clears the derived color overrides. **Actuator nodes** fold into
/// the same desired-override map (a placeable dataflow output port), so one
/// change-detected apply/cleanup covers bindings and actuators alike. Runs
/// every frame (cheap); history recording pauses with the simulation.
pub fn evaluate_signals(
    mut commands: Commands,
    bindings: Res<SignalBindings>,
    params: Res<SignalParams>,
    computed: Res<ComputedSignals>,
    script_names: Res<ScriptSignals>,
    index: Res<IdIndex>,
    transforms: Query<&Transform, With<Body>>,
    physics: PhysicsQueries,
    fixed: Res<Time<Fixed>>,
    game: Res<State<GameState>>,
    mut bus: ResMut<SignalBus>,
    mut overrides: Query<&mut SignalColorOverride, With<Body>>,
    overridden: Query<Entity, With<SignalColorOverride>>,
) {
    let recording = *game.get() == GameState::Playing;
    let dt = fixed.timestep().as_secs_f32().max(1e-6);

    // Evaluate sources and gather the desired per-entity overrides.
    let mut desired: HashMap<Entity, SignalColorOverride> = HashMap::new();
    for binding in &bindings.0 {
        let value = match &binding.source {
            SignalSource::Named(name) => bus.get(name),
            other => read_source(other, &index, &physics, &transforms, dt),
        };
        let Some(value) = value else {
            continue;
        };
        bus.publish(&binding.name, value, recording);

        let color = binding.gradient.at(binding.transfer(value));
        match &binding.sink {
            SignalSink::Fill(id) => {
                if let Some(entity) = index.entity(*id) {
                    desired.entry(entity).or_default().fill = Some(color);
                }
            }
            SignalSink::TracerColor(id) => {
                if let Some(entity) = index.entity(*id) {
                    desired.entry(entity).or_default().trail = Some(color);
                }
            }
            SignalSink::Plot => {}
        }
    }

    // Bus hygiene: keep the names that still have a producer — bindings,
    // params, computed signals, `t`, script names, and the canonical sensor
    // names `publish_sensor_refs` surfaces for a computed's operands (a body
    // sensor feeding a modulation block has no binding/param producer, only
    // the reference from a computed's expression).
    bus.retain(|name| {
        name == TIME_INPUT
            || bindings.0.iter().any(|b| b.name == name)
            || params.0.iter().any(|p| p.name == name)
            || computed
                .0
                .iter()
                .any(|c| c.name == name || c.expr.inputs().iter().any(|i| i == name))
            || script_names.0.iter().any(|n| n == name)
    });

    // Apply overrides change-detected (a same-color frame must not dirty
    // `Changed<SignalColorOverride>` and churn materials).
    for (entity, next) in &desired {
        match overrides.get_mut(*entity) {
            Ok(mut current) => {
                if *current != *next {
                    *current = *next;
                }
            }
            Err(_) => {
                commands.entity(*entity).insert(*next);
            }
        }
    }
    for entity in &overridden {
        if !desired.contains_key(&entity) {
            commands.entity(entity).remove::<SignalColorOverride>();
        }
    }
}

/// Installs the signal dataflow: the bindings/bus resources and the
/// per-frame evaluator (headless too — tests drive it directly).
#[derive(Default)]
pub struct SignalPlugin;

impl Plugin for SignalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SignalBindings>();
        app.init_resource::<SignalParams>();
        app.init_resource::<ComputedSignals>();
        app.init_resource::<CompiledSignals>();
        app.init_resource::<SignalBus>();
        app.init_resource::<ScriptSignals>();
        app.register_type::<SignalBindings>();
        app.register_type::<SignalParams>();
        app.register_type::<ComputedSignals>();
        // Recompile when the authored computed set changes (and once at
        // startup via the initial change tick); evaluate every frame,
        // computed modulators before the bindings that read them.
        app.add_systems(
            Update,
            (
                recompile_signals.run_if(resource_changed::<ComputedSignals>),
                publish_sensor_refs,
                evaluate_computed,
                evaluate_signals,
            )
                .chain(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_publishes_records_and_retains() {
        let mut bus = SignalBus::default();
        bus.publish("a", 1.0, true);
        bus.publish("a", 2.0, true);
        bus.publish("b", 9.0, false);
        assert_eq!(bus.get("a"), Some(2.0));
        assert_eq!(bus.get("b"), Some(9.0));
        let a = bus.entries().next().unwrap().1;
        assert_eq!(a.history().len(), 2, "recorded samples");
        assert_eq!(
            bus.entries().nth(1).unwrap().1.history().len(),
            0,
            "record=false keeps value only"
        );
        bus.retain(|n| n == "a");
        assert_eq!(bus.get("b"), None);
        assert_eq!(bus.get("a"), Some(2.0));
    }

    #[test]
    fn bus_history_is_capped() {
        let mut bus = SignalBus::default();
        for i in 0..(HISTORY_CAP + 50) {
            bus.publish("s", i as f32, true);
        }
        let entry = bus.entries().next().unwrap().1;
        assert_eq!(entry.history().len(), HISTORY_CAP);
    }
}
