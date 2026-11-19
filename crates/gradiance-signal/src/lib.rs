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
//!   like `SimSettings`): the UI edits it directly, it persists with the
//!   scene, it is scene content (a settled edit is one `settings` undo step),
//!   and it references bodies only by [`StableId`].
//! - The per-frame evaluator is plain queries + arithmetic — the scripting
//!   VM stays off the hot path. Scripts participate by *publishing* named
//!   values on their own (cold) runs via `signal-set`.
//!
//! [`Appearance`]: gradiance_domain::appearance::Appearance

pub mod compile;

use avian2d::prelude::Physics;
use bevy::ecs::system::SystemState;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_core::states::GameState;
use gradiance_domain::quantity::{BodySample, JointSample, QuantitySamples, Subject};
use gradiance_domain::signal::Subjects;
use gradiance_domain::{Body, Joint};
use gradiance_physics::drive::SignalDrive;
use gradiance_physics::queries::PhysicsQueries;
use std::collections::VecDeque;

pub use gradiance_domain::signal::{
    ActuatorTarget, BlockOp, ComputedSignal, ComputedSignals, Curve, CurveInterp, GradientSpec,
    SignalBinding, SignalBindings, SignalExpr, SignalMap, SignalParam, SignalParams, SignalSink,
    SignalSource, key_input_name, key_of_input, remap_binding,
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
#[derive(Resource, Debug)]
pub struct SignalBus {
    entries: Vec<(String, BusEntry)>,
    now: f32,
    cap: usize,
}

impl Default for SignalBus {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            now: 0.0,
            cap: HISTORY_CAP,
        }
    }
}

impl SignalBus {
    /// Samples kept per signal before the oldest is dropped: 600 by default
    /// (the plot window). The headless runner (`gradiance run`, brief R-2)
    /// lifts it to `usize::MAX` so a rollout keeps every step: its recording
    /// is bounded by the steps it was asked for, not by a window.
    pub fn set_history_cap(&mut self, cap: usize) {
        self.cap = cap.max(1);
    }

    /// The samples kept per signal (see [`set_history_cap`](Self::set_history_cap)).
    pub fn history_cap(&self) -> usize {
        self.cap
    }

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
            while entry.history.len() > self.cap {
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
    computed: Res<CompiledSignals>,
    index: Res<IdIndex>,
    physics: PhysicsQueries,
    fixed: Res<Time<Fixed>>,
    sim: Res<Time<Physics>>,
    game: Res<State<GameState>>,
    step: Res<gradiance_physics::SimStep>,
    transforms: Query<&Transform, With<Body>>,
    mut bus: ResMut<SignalBus>,
) {
    // First publisher of the frame, so this is where the frame's recording
    // timestamp is set — every sample recorded below and downstream shares it.
    bus.set_time(sim.elapsed_secs());
    let recording = *game.get() == GameState::Playing || step.active();
    let dt = fixed.timestep().as_secs_f32().max(1e-6);
    // The compiled input lists, not `expr.inputs()`: those were collected
    // once at compile time, so this pass allocates nothing.
    for (_, signal) in &computed.compiled {
        for input in &signal.inputs {
            if let Some(source) = SignalSource::from_bus_name(input)
                && let Some(value) = read_source(&source, &index, &physics, &transforms, dt)
            {
                bus.publish(input, value, recording);
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
    step: Res<gradiance_physics::SimStep>,
    mut bus: ResMut<SignalBus>,
) {
    let recording = *game.get() == GameState::Playing || step.active();
    for param in &params.0 {
        bus.publish(&param.name, param.value, recording);
    }
    bus.publish(TIME_INPUT, time.elapsed_secs(), recording);
    // A stack array, not a `Vec`: a compiled signal has at most `MAX_VARS`
    // inputs (the compiler refuses more), so the frame loop allocates nothing.
    let mut vars = [0.0_f32; gradiance_kernel::MAX_VARS];
    for (name, signal) in &computed.compiled {
        let n = signal.inputs.len().min(vars.len());
        for (slot, input) in vars.iter_mut().zip(&signal.inputs) {
            *slot = bus.get(input).unwrap_or(0.0);
        }
        let value = signal.kernel.eval(&vars[..n]);
        bus.publish(name, value, recording);
    }
}

/// Reads a scene [`SignalSource`] to a scalar — the single shared reader the
/// binding evaluator and the inspector's sensor-port readouts both use (one
/// reader, no parallel implementations). The source names a row of the
/// quantity catalog (`gradiance_domain::quantity`), whose reader runs over the
/// `physics::queries` facade ([`PhysicsQueries::body`] / [`joint`]) — one
/// trait call per read, allocation-free. [`SignalSource::Named`] is bus-only
/// (resolved by the caller against the [`SignalBus`]). `None` when a
/// referenced body isn't resolvable this frame.
///
/// [`joint`]: PhysicsQueries::joint
pub fn read_source(
    source: &SignalSource,
    index: &IdIndex,
    physics: &PhysicsQueries,
    transforms: &Query<&Transform, With<Body>>,
    dt: f32,
) -> Option<f32> {
    let (row, subjects) = source.reading()?;
    let body = |id: StableId| {
        let entity = index.entity(id)?;
        let position = transforms
            .get(entity)
            .ok()
            .map(|t| t.translation.truncate());
        Some(physics.body(entity, position, dt))
    };
    match (row.subject(), subjects) {
        (Subject::Body, Subjects::One(id)) => row.read_body(&body(id)?),
        (Subject::BodyPair, Subjects::Two(a, b)) => row.read_pair(&body(a)?, &body(b)?),
        (Subject::Joint, Subjects::One(id)) => {
            row.read_joint(&physics.joint(index.entity(id)?, index))
        }
        _ => None,
    }
}

/// Captures every body's and joint's catalog reads once
/// ([`QuantitySamples`]): the mirror the `quantity` operations read, so a
/// script run sees one snapshot (scripting rule 3). Empty in a world without
/// the physics engine's resources (a headless fixture).
pub fn sample_quantities(world: &mut World) -> QuantitySamples {
    if !world.contains_resource::<avian2d::prelude::ContactGraph>()
        || !world.contains_resource::<IdIndex>()
    {
        return QuantitySamples::default();
    }
    let dt = world
        .get_resource::<Time<Fixed>>()
        .map_or(1.0 / 60.0, |t| t.timestep().as_secs_f32())
        .max(1e-6);
    let mut state: SystemState<(
        PhysicsQueries,
        Res<IdIndex>,
        Query<(Entity, &StableId, Option<&Transform>), With<Body>>,
        Query<(Entity, &StableId), With<Joint>>,
    )> = SystemState::new(world);
    let Ok((physics, index, bodies, joints)) = state.get(world) else {
        return QuantitySamples::default();
    };
    QuantitySamples {
        bodies: bodies
            .iter()
            .map(|(entity, id, transform)| {
                let position = transform.map(|t| t.translation.truncate());
                (
                    *id,
                    BodySample::capture(&physics.body(entity, position, dt)),
                )
            })
            .collect(),
        joints: joints
            .iter()
            .map(|(entity, id)| (*id, JointSample::capture(&physics.joint(entity, &index))))
            .collect(),
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
    compiled: Res<CompiledSignals>,
    script_names: Res<ScriptSignals>,
    index: Res<IdIndex>,
    transforms: Query<&Transform, With<Body>>,
    physics: PhysicsQueries,
    fixed: Res<Time<Fixed>>,
    game: Res<State<GameState>>,
    step: Res<gradiance_physics::SimStep>,
    mut bus: ResMut<SignalBus>,
    mut overrides: Query<&mut SignalColorOverride, With<Body>>,
    overridden: Query<Entity, With<SignalColorOverride>>,
) {
    let recording = *game.get() == GameState::Playing || step.active();
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
            // Plot publishes only; an actuator's value is read off the bus
            // by `drive_actuators`, after this pass.
            SignalSink::Plot
            | SignalSink::MotorSpeed(_)
            | SignalSink::MotorCap(_)
            | SignalSink::FieldStrength(_)
            | SignalSink::GravityScale(_)
            | SignalSink::Thrust(_) => {}
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
            || computed.0.iter().any(|c| c.name == name)
            || compiled
                .compiled
                .iter()
                .any(|(_, c)| c.inputs.iter().any(|i| i == name))
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
            // `try_`: this system is unordered against the command
            // dispatcher, so an undo/delete can despawn the body between
            // queuing this insert and applying it. A derived override on a
            // dead body is moot — skip it rather than panic (the #223 CI
            // flake, pinned by `nodes::a_body_despawned_while_its_override_
            // is_queued_does_not_panic`).
            Err(_) => {
                commands.entity(*entity).try_insert(*next);
            }
        }
    }
    for entity in &overridden {
        if !desired.contains_key(&entity) {
            commands.entity(entity).try_remove::<SignalColorOverride>();
        }
    }
}

/// The [`SignalDrive`] the bindings ask of target `id`: each actuator binding
/// whose sink names `id` sets its field to the binding's value on the bus
/// (published this frame by [`evaluate_signals`]). A binding with no value
/// yet, or a non-finite one, sets nothing: a NaN must never reach the solver.
/// When two bindings drive the same field, the later one wins (authored
/// order, like a cycle's last-frame rule). Pure over its inputs; allocates
/// nothing.
pub fn desired_drive(id: StableId, bindings: &[SignalBinding], bus: &SignalBus) -> SignalDrive {
    let mut drive = SignalDrive::default();
    for binding in bindings {
        if binding.sink.actuator().map(ActuatorTarget::id) != Some(id) {
            continue;
        }
        let Some(value) = bus.get(&binding.name).filter(|v| v.is_finite()) else {
            continue;
        };
        let slot = match binding.sink {
            SignalSink::MotorSpeed(_) => &mut drive.motor_speed,
            SignalSink::MotorCap(_) => &mut drive.motor_cap,
            SignalSink::FieldStrength(_) => &mut drive.field_strength,
            SignalSink::GravityScale(_) => &mut drive.gravity_scale,
            SignalSink::Thrust(_) => &mut drive.thrust,
            SignalSink::Fill(_) | SignalSink::TracerColor(_) | SignalSink::Plot => continue,
        };
        *slot = Some(value);
    }
    drive
}

/// **The actuator pass** (brief N-1): one system over the bindings and the
/// [`SignalDrive`] query. Each driven entity's drive is recomputed
/// ([`desired_drive`]) and written only on a difference; a drive no binding
/// sets any more is removed (the authored value returns with no command); a
/// bound target without one gets it. Runs after [`evaluate_signals`], so it
/// reads this frame's values; the physics layer applies the drives in
/// `PostUpdate`, before the next step.
///
/// Cost: `O(D·B + A·B)` id comparisons for `D` live drives, `A`
/// actuator bindings and `B` bindings, plus one bus lookup (a scan of the bus names) per match, with no
/// allocation in the steady state (a command is queued only when a drive
/// appears or goes).
pub fn drive_actuators(
    mut commands: Commands,
    bindings: Res<SignalBindings>,
    bus: Res<SignalBus>,
    index: Res<IdIndex>,
    mut drives: Query<(Entity, &StableId, &mut SignalDrive)>,
) {
    for (entity, id, mut drive) in &mut drives {
        let next = desired_drive(*id, &bindings.0, &bus);
        if next.is_empty() {
            commands.entity(entity).try_remove::<SignalDrive>();
        } else if *drive != next {
            *drive = next;
        }
    }
    for binding in &bindings.0 {
        let Some(target) = binding.sink.actuator() else {
            continue;
        };
        let Some(entity) = index.entity(target.id()) else {
            continue;
        };
        if drives.contains(entity) {
            continue;
        }
        let next = desired_drive(target.id(), &bindings.0, &bus);
        if !next.is_empty() {
            // `try_`: the target may be despawned before the queue applies
            // (the same race as the colour override's).
            commands.entity(entity).try_insert(next);
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
                drive_actuators,
            )
                .chain()
                // A step request that lands this frame is seen by every
                // recorder or by none (brief R-2).
                .after(gradiance_physics::TransportSet),
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

    fn actuator(name: &str, sink: SignalSink) -> SignalBinding {
        SignalBinding {
            name: name.into(),
            source: SignalSource::Named(name.into()),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink,
        }
    }

    #[test]
    fn a_drive_holds_only_the_fields_bound_to_its_target() {
        let (wheel, ball) = (StableId::new(), StableId::new());
        let bindings = [
            actuator("spin", SignalSink::MotorSpeed(wheel)),
            actuator("cap", SignalSink::MotorCap(wheel)),
            actuator("float", SignalSink::GravityScale(ball)),
            actuator("tint", SignalSink::Fill(wheel)),
            actuator("pending", SignalSink::FieldStrength(wheel)),
        ];
        let mut bus = SignalBus::default();
        bus.publish("spin", 3.0, false);
        bus.publish("cap", f32::NAN, false);
        bus.publish("float", 0.25, false);
        bus.publish("tint", 9.0, false);
        let drive = desired_drive(wheel, &bindings, &bus);
        assert_eq!(drive.motor_speed, Some(3.0));
        assert_eq!(drive.motor_cap, None, "a NaN never reaches the solver");
        assert_eq!(drive.field_strength, None, "no value on the bus yet");
        assert_eq!(drive.gravity_scale, None, "another target's field");
        let ball_drive = desired_drive(ball, &bindings, &bus);
        assert_eq!(ball_drive.gravity_scale, Some(0.25));
        assert!(desired_drive(StableId::new(), &bindings, &bus).is_empty());
    }

    #[test]
    fn a_thrust_binding_drives_its_thruster_node() {
        let (thruster, wheel) = (StableId::new(), StableId::new());
        let bindings = [
            actuator("burn", SignalSink::Thrust(thruster)),
            actuator("spin", SignalSink::MotorSpeed(wheel)),
        ];
        let mut bus = SignalBus::default();
        bus.publish("burn", 12.5, false);
        bus.publish("spin", 3.0, false);
        let drive = desired_drive(thruster, &bindings, &bus);
        assert_eq!(drive.thrust, Some(12.5));
        assert_eq!(drive.motor_speed, None, "another target's field");
        assert_eq!(desired_drive(wheel, &bindings, &bus).thrust, None);
    }

    #[test]
    fn a_lifted_cap_keeps_every_sample() {
        let mut bus = SignalBus::default();
        bus.set_history_cap(usize::MAX);
        for i in 0..(HISTORY_CAP * 2) {
            bus.publish("s", i as f32, true);
        }
        let entry = bus.entries().next().unwrap().1;
        assert_eq!(entry.history().len(), HISTORY_CAP * 2);
        assert_eq!(entry.times().len(), HISTORY_CAP * 2);
        bus.set_history_cap(0);
        assert_eq!(bus.history_cap(), 1, "at least one sample is kept");
    }
}
