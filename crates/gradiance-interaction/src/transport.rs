//! The simulation transport (brief R-1): play, pause, step and the physics
//! clock, as operations (`sim.play`, `sim.pause`, `sim.step`, `sim.time`).
//!
//! The Space key, the toolbar's play button and the operations all write one
//! [`SimTransport`] request, and [`apply_sim_transport`] is the one system
//! that turns requests into a [`GameState`] change and owed steps
//! ([`SimStep`]). Starting or stepping a run is not a world mutation (sim
//! state is derived, CLAUDE.md invariant 5), so no request is a command and
//! none is undone: the same reasoning as the pack session's
//! `StartPackRequest`.
//!
//! `sim.step` returns at once, like `ui.drag`: the steps run over the next
//! fixed ticks, and a client reads `sim.time` to see them done.

use bevy::prelude::*;
use gradiance_core::states::GameState;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, json,
};
use gradiance_physics::SimStep;
use serde::Deserialize;

/// The most fixed steps one `sim.step` may ask for (an hour at 60 Hz): more
/// is a typo, not an experiment.
pub const MAX_STEPS: u32 = 216_000;

/// One transport request, applied in order by [`apply_sim_transport`].
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimTransport {
    /// Run the simulation (drops any owed steps).
    Play,
    /// Freeze the simulation (drops any owed steps).
    Pause,
    /// Play when paused, pause when playing (the Space key).
    Toggle,
    /// Pause, then advance exactly this many fixed steps.
    Step(u32),
}

/// The transport as the operations read it: a context mirror.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimClock {
    /// Simulated seconds on the physics clock.
    pub time: f64,
    /// Whether the simulation plays.
    pub playing: bool,
    /// Fixed steps a `sim.step` still owes.
    pub steps_left: u32,
}

/// Registers the transport operations, the clock mirror and the request.
pub(crate) fn install(app: &mut App) {
    app.add_message::<SimTransport>();
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<SimPlay>();
        registry.register::<SimPause>();
        registry.register::<SimStepOp>();
        registry.register::<SimTime>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_clock_mirror);
    // After the invocations (an operation's request lands the frame it is
    // invoked) and after the shortcuts (the Space key's).
    app.add_systems(
        Update,
        apply_sim_transport
            .in_set(gradiance_physics::TransportSet)
            .after(gradiance_op::OpInvokeSet)
            .after(crate::InteractionSet),
    );
}

/// Publishes the physics clock and the transport state.
fn fill_clock_mirror(world: &mut World, ctx: &mut OpContext) {
    let time = world
        .get_resource::<Time<avian2d::prelude::Physics>>()
        .map_or(0.0, Time::elapsed_secs_f64);
    let playing = world
        .get_resource::<State<GameState>>()
        .is_some_and(|state| *state.get() == GameState::Playing);
    let steps_left = world
        .get_resource::<SimStep>()
        .map_or(0, SimStep::remaining);
    ctx.insert(SimClock {
        time,
        playing,
        steps_left,
    });
}

/// Applies this frame's transport requests in order: the last word on
/// playing wins, a step pauses first, and play or pause drops owed steps.
pub fn apply_sim_transport(
    mut requests: MessageReader<SimTransport>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut step: ResMut<SimStep>,
) {
    let now = *state.get() == GameState::Playing;
    let mut playing = now;
    let mut any = false;
    for request in requests.read() {
        any = true;
        match *request {
            SimTransport::Play => {
                playing = true;
                step.cancel();
            }
            SimTransport::Pause => {
                playing = false;
                step.cancel();
            }
            SimTransport::Toggle => {
                playing = !playing;
                step.cancel();
            }
            SimTransport::Step(n) => {
                playing = false;
                step.request(n);
            }
        }
    }
    if any && playing != now {
        next.set(if playing {
            GameState::Playing
        } else {
            GameState::Paused
        });
    }
}

/// The metadata of a transport operation: editor state, no undo step.
const fn transport(
    id: &'static str,
    lisp: &'static str,
    doc: &'static str,
    example: &'static str,
) -> OpMeta {
    OpMeta {
        id,
        lisp,
        doc,
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example,
    }
}

/// Run the simulation.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimPlay {}

impl Operation for SimPlay {
    const META: OpMeta = transport(
        "sim.play",
        "sim-play",
        "Run the simulation (the transport's Play; drops steps a sim-step still owes). No undo step.",
        "{}",
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(SimTransport::Play))
    }
}

/// Freeze the simulation.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimPause {}

impl Operation for SimPause {
    const META: OpMeta = transport(
        "sim.pause",
        "sim-pause",
        "Freeze the simulation (the transport's Pause; drops steps a sim-step still owes). No undo step.",
        "{}",
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(SimTransport::Pause))
    }
}

/// Pause, then advance exactly `n` fixed steps.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimStepOp {
    /// How many fixed steps (1 to [`MAX_STEPS`]).
    pub n: u32,
}

impl Operation for SimStepOp {
    const META: OpMeta = transport(
        "sim.step",
        "sim-step",
        "Pause, then advance exactly n fixed physics steps (n / timestep_hz seconds at speed 1). Returns at once; read sim-time to see the steps done. No undo step.",
        r#"{"n": 60}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        if self.n == 0 || self.n > MAX_STEPS {
            return Err(OpError::Invalid(format!(
                "n must be 1 to {MAX_STEPS}, got {}",
                self.n
            )));
        }
        Ok(OpOutput::none().message(SimTransport::Step(self.n)))
    }
}

/// Simulated seconds on the physics clock.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimTime {}

impl Operation for SimTime {
    const META: OpMeta = OpMeta {
        id: "sim.time",
        lisp: "sim-time",
        doc: "Simulated seconds on the physics clock (frozen while paused; advanced by sim-step).",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let clock = ctx.get::<SimClock>()?;
        Ok(OpOutput::none().returning(json!(clock.time)))
    }
}
