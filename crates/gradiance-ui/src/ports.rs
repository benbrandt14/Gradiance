//! Object **ports**: the per-object sensor (read-only) and actuator
//! (writable) data the inspector surfaces and the node canvas wires.
//!
//! The Algodoo/Simulink framing: a body is not a bespoke panel of unrelated
//! widgets — it exposes named **ports**. A **sensor** port is a read-only live
//! quantity (a [`SignalSource`] scene read: height, speed, contact force); an
//! **actuator** port is a writable channel (a [`SignalSink`]: fill / tracer
//! color). One curated catalog, reused by the inspector readouts and the
//! node-graph canvas, read through the single
//! [`read_source`](gradiance_signal::read_source) reader — no parallel "read a
//! body quantity" implementation.

use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::Body;
use gradiance_domain::signal::{
    GradientSpec, SignalBinding, SignalBindings, SignalMap, SignalSink, SignalSource,
};
use gradiance_physics::queries::PhysicsQueries;
use gradiance_units::Mass;

/// A body's **sensor ports** as `(label, source)` — the read-only scene reads
/// it publishes. The single catalog behind the inspector readouts and the
/// canvas output pins.
pub fn body_sensors(id: StableId) -> [(&'static str, SignalSource); 6] {
    [
        ("speed", SignalSource::Speed(id)),
        ("spin", SignalSource::Spin(id)),
        ("height", SignalSource::Height(id)),
        ("pos x", SignalSource::PosX(id)),
        ("force", SignalSource::ContactForce(id)),
        ("contacts", SignalSource::ContactCount(id)),
    ]
}

/// A body's **actuator ports** as `(label, sink)` — the writable color
/// channels a bound signal can drive. The catalog behind the canvas input pins.
pub fn body_actuators(id: StableId) -> [(&'static str, SignalSink); 2] {
    [
        ("fill", SignalSink::Fill(id)),
        ("tracer", SignalSink::TracerColor(id)),
    ]
}

/// A short unit label for a sensor source's readout.
pub fn unit(source: &SignalSource) -> &'static str {
    match source {
        SignalSource::Speed(_) => "px/s",
        SignalSource::Spin(_) => "rad/s",
        SignalSource::Height(_) | SignalSource::PosX(_) | SignalSource::Distance(..) => "px",
        SignalSource::ContactForce(_) => "N",
        SignalSource::ContactCount(_) | SignalSource::Named(_) => "",
    }
}

/// Renders a body's sensor ports as live read-only rows (name → value → a
/// **plot** toggle) plus its derived mass — the read-only half of "every datum
/// is a port". The plot toggle adds/removes a `SignalSink::Plot` binding for
/// the source, so the Live Plot draws it (wiring a sensor to the plot sink).
/// `dt` scales the impulse→force readout (matching the binding evaluator).
pub fn sensor_readouts(
    ui: &mut egui::Ui,
    id: StableId,
    index: &IdIndex,
    physics: &PhysicsQueries,
    transforms: &Query<&Transform, With<Body>>,
    dt: f32,
    bindings: &mut SignalBindings,
) {
    for (label, source) in body_sensors(id) {
        ui.horizontal(|ui| {
            ui.label(label);
            match gradiance_signal::read_source(&source, index, physics, transforms, dt) {
                Some(value) => ui.monospace(format!("{value:.2} {}", unit(&source))),
                None => ui.weak("—"),
            };
            let plotted = plot_index(bindings, &source).is_some();
            if ui
                .selectable_label(plotted, "▸plot")
                .on_hover_text("plot this sensor over time (a plot-sink binding)")
                .clicked()
            {
                match plot_index(bindings, &source) {
                    Some(i) => {
                        bindings.0.remove(i);
                    }
                    None => bindings.0.push(SignalBinding {
                        name: format!("{label}@{id:.4}"),
                        source: source.clone(),
                        map: SignalMap::default(),
                        curve: None,
                        gradient: GradientSpec::default(),
                        sink: SignalSink::Plot,
                    }),
                }
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("mass");
        match index.entity(id).and_then(|e| physics.mass_of(e)) {
            Some(mass) => ui.monospace(format!("{:.2} {}", mass.value(), Mass::UNIT)),
            None => ui.weak("—"),
        };
    });
}

/// Index of an existing plot-sink binding for `source`, if any.
fn plot_index(bindings: &SignalBindings, source: &SignalSource) -> Option<usize> {
    bindings
        .0
        .iter()
        .position(|b| b.source == *source && b.sink == SignalSink::Plot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_ports_are_distinct_and_labelled() {
        let id = StableId::new();
        let sensor_labels: Vec<&str> = body_sensors(id).iter().map(|(l, _)| *l).collect();
        let mut deduped = sensor_labels.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(deduped.len(), sensor_labels.len(), "duplicate sensor ports");
        // Every source has a unit (exhaustive match forces a new variant here).
        for (_, source) in body_sensors(id) {
            let _ = unit(&source);
        }
        assert_eq!(body_actuators(id).len(), 2, "fill + tracer actuator ports");
    }
}
