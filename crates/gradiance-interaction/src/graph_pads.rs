//! Gamepad sticks and buttons as node-graph inputs (brief S-20): the pad
//! counterpart of [`graph_keys`](crate::graph_keys).
//!
//! A pad input is a `BlockOp::Pad` block in the authored `ComputedSignals`
//! (scene content: saved, one `settings` undo step per edit). Its
//! expression reads the bus input `pad@<name>[/<dead zone>]`;
//! [`publish_graph_pads`] writes the value there each frame, just before
//! the kernels run: a stick −1..1, a button 0..1, past the dead zone. Those
//! values are **derived**: never saved, never a command, never an undo
//! step.
//!
//! **Which gamepad:** the first `Gamepad` entity the query yields. A second
//! pad is not addressable yet (one person plays a scene at a time).
//!
//! **The gate:** a pad input reads `0` unless the sim plays, like a key
//! input. A pad has no editor use, so it never yields to a text field.
//!
//! **Cost per frame:** one `Gamepad` read and one bus publish per pad
//! reference. Nothing allocates: the reference list is rebuilt only when
//! the authored signals change.

use bevy::prelude::*;
use gradiance_core::states::GameState;
use gradiance_domain::pad::{PadSource, apply_dead_zone, parse_pad_input};
use gradiance_domain::signal::ComputedSignals;
use gradiance_signal::SignalBus;

/// The bus inputs the node graph's pad blocks read. Derived from
/// `ComputedSignals` on change; never saved.
#[derive(Resource, Debug, Default)]
pub struct GraphPads {
    /// Each `pad@…` bus input a kernel reads, with what it reads and its
    /// dead zone.
    refs: Vec<(String, PadSource, f32)>,
}

impl GraphPads {
    /// Rebuilds the reference list from the authored signals (cold: runs
    /// when they change).
    pub fn rebuild(&mut self, computed: &ComputedSignals) {
        self.refs.clear();
        for signal in &computed.0 {
            for input in signal.expr.inputs() {
                let Some((pad, dead_zone)) = parse_pad_input(&input) else {
                    continue;
                };
                if !self.refs.iter().any(|(name, ..)| *name == input) {
                    self.refs.push((input, pad.source, dead_zone));
                }
            }
        }
    }

    /// How many bus inputs the pads feed.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.refs.len()
    }

    /// Whether no pad block is authored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }
}

/// Installs the graph pads beside the graph keys: the reference list
/// follows the authored signals, and the values land on the bus between
/// the frame's first publisher and the kernels that read them.
pub(crate) fn install(app: &mut App) {
    app.init_resource::<GraphPads>();
    app.add_systems(
        Update,
        (
            refresh_graph_pads.run_if(resource_exists_and_changed::<ComputedSignals>),
            publish_graph_pads.run_if(resource_exists::<SignalBus>),
        )
            .chain()
            .after(gradiance_signal::publish_sensor_refs)
            .before(gradiance_signal::evaluate_computed),
    );
}

/// Rebuilds [`GraphPads`] when the authored signals change.
pub fn refresh_graph_pads(computed: Res<ComputedSignals>, mut pads: ResMut<GraphPads>) {
    pads.rebuild(&computed);
}

/// Publishes each pad reference's value on the bus for the kernels that
/// read it: the first gamepad's stick or button past its dead zone, `0`
/// with no gamepad or while the sim does not play.
pub fn publish_graph_pads(
    graph: Res<GraphPads>,
    gamepads: Query<&Gamepad>,
    game: Res<State<GameState>>,
    step: Res<gradiance_physics::SimStep>,
    mut bus: ResMut<SignalBus>,
) {
    if graph.refs.is_empty() {
        return;
    }
    let playing = *game.get() == GameState::Playing;
    let recording = playing || step.active();
    let gamepad = gamepads.iter().next().filter(|_| playing);
    for (name, source, dead_zone) in &graph.refs {
        let raw = gamepad.and_then(|g| match *source {
            PadSource::Axis(axis) => g.get(axis),
            PadSource::Button(button) => g.get(button),
        });
        bus.publish(
            name,
            apply_dead_zone(raw.unwrap_or(0.0), *dead_zone),
            recording,
        );
    }
}
