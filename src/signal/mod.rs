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
//! [`Appearance`]: crate::domain::appearance::Appearance

use crate::core::ids::{IdIndex, StableId};
use crate::core::states::GameState;
use crate::domain::Body;
use crate::physics::queries::PhysicsQueries;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::collections::VecDeque;

pub use crate::domain::signal::{
    GradientSpec, SignalBinding, SignalBindings, SignalMap, SignalSink, SignalSource,
};

/// Samples of history kept per signal (~10 s at 60 fps), matching the
/// plot panel's window.
const HISTORY_CAP: usize = 600;

/// One live signal on the bus: its current value and rolling history.
#[derive(Debug, Default)]
pub struct BusEntry {
    value: f32,
    history: VecDeque<f32>,
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
}

/// The live signal bus: named current values + rolling histories. Derived
/// state — rebuilt continuously, never persisted. Scripts publish into it
/// with `signal-set` and read it with `signal-get`; the evaluator publishes
/// every binding; the plot panel draws the histories.
#[derive(Resource, Debug, Default)]
pub struct SignalBus {
    entries: Vec<(String, BusEntry)>,
}

impl SignalBus {
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
            while entry.history.len() > HISTORY_CAP {
                entry.history.pop_front();
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

/// Evaluates every binding: reads its source, publishes to the bus, and
/// writes/clears the derived color overrides. Runs every frame (cheap —
/// a handful of bindings); history recording pauses with the simulation.
pub fn evaluate_signals(
    mut commands: Commands,
    bindings: Res<SignalBindings>,
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
    let pos_of = |id: StableId| -> Option<Vec2> {
        index
            .entity(id)
            .and_then(|e| transforms.get(e).ok())
            .map(|t| t.translation.truncate())
    };

    // Evaluate sources and gather the desired per-entity overrides.
    let mut desired: HashMap<Entity, SignalColorOverride> = HashMap::new();
    for binding in &bindings.0 {
        let value = match &binding.source {
            SignalSource::Speed(id) => index
                .entity(*id)
                .and_then(|e| physics.velocity_of(e))
                .map(|(v, _)| v.length()),
            SignalSource::Spin(id) => index
                .entity(*id)
                .and_then(|e| physics.velocity_of(e))
                .map(|(_, w)| w.abs()),
            SignalSource::Height(id) => pos_of(*id).map(|p| p.y),
            SignalSource::Distance(a, b) => match (pos_of(*a), pos_of(*b)) {
                (Some(pa), Some(pb)) => Some(pa.distance(pb)),
                _ => None,
            },
            SignalSource::ContactForce(id) => index
                .entity(*id)
                .map(|e| physics.net_contact_impulse(e).length() / dt),
            SignalSource::ContactCount(id) => {
                index.entity(*id).map(|e| physics.touching_count(e) as f32)
            }
            SignalSource::Named(name) => bus.get(name),
        };
        let Some(value) = value else {
            continue;
        };
        bus.publish(&binding.name, value, recording);

        let color = binding.gradient.at(binding.map.normalize(value));
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

    // Bus hygiene: keep bound names and script-published names; drop the rest.
    bus.retain(|name| {
        bindings.0.iter().any(|b| b.name == name) || script_names.0.iter().any(|n| n == name)
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
        app.init_resource::<SignalBus>();
        app.init_resource::<ScriptSignals>();
        app.register_type::<SignalBindings>();
        app.add_systems(Update, evaluate_signals);
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
