//! Object **ports**: the per-object sensor (read-only) and actuator
//! (writable) data the inspector surfaces and the node canvas wires.
//!
//! The Algodoo/Simulink framing: a body is not a bespoke panel of unrelated
//! widgets — it exposes named **ports**. A **sensor** port is a read-only live
//! quantity (a [`SignalSource`] scene read: height, speed, contact force); an
//! **actuator** port is a writable channel (a [`SignalSink`]: fill / tracer
//! color). The sensors are the quantity catalog's rows
//! (`gradiance_domain::quantity`, brief 5-2), reused by the inspector
//! readouts and the node-graph canvas, read through the single
//! [`read_source`](gradiance_signal::read_source) reader — no parallel "read a
//! body quantity" implementation.

use crate::widgets;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::Body;
use gradiance_domain::quantity::{Subject, quantities_of, quantity};
use gradiance_domain::signal::{
    GradientSpec, SignalBinding, SignalBindings, SignalMap, SignalSink, SignalSource, Subjects,
};
use gradiance_physics::queries::PhysicsQueries;
use gradiance_units::Dimension;

/// A body's **sensor ports** as `(label, source)` — the read-only scene reads
/// it publishes: every body row of the quantity catalog
/// (`gradiance_domain::quantity`) that a [`SignalSource`] can bind, in catalog
/// order, labelled from the row. The single list behind the inspector
/// readouts and the canvas output pins.
pub fn body_sensors(id: StableId) -> Vec<(&'static str, SignalSource)> {
    sensors_of(Subject::Body, id)
}

/// A joint's **sensor ports** as `(label, source)` — its live length and
/// angle (`JointDef::sensor_state`) and its linear reaction force (avian's
/// `JointForces`): the catalog's joint rows. The list behind the joint
/// inspector's readouts.
pub fn joint_sensors(id: StableId) -> Vec<(&'static str, SignalSource)> {
    sensors_of(Subject::Joint, id)
}

/// The catalog rows read off `subject` that a source can bind, as ports.
fn sensors_of(subject: Subject, id: StableId) -> Vec<(&'static str, SignalSource)> {
    quantities_of(subject)
        .filter_map(|q| {
            Some((
                q.label,
                SignalSource::of_quantity(q.name, Subjects::One(id))?,
            ))
        })
        .collect()
}

/// A body's **actuator ports** as `(label, sink)` — the writable color
/// channels a bound signal can drive. The catalog behind the canvas input pins.
pub fn body_actuators(id: StableId) -> [(&'static str, SignalSink); 2] {
    [
        ("fill", SignalSink::Fill(id)),
        ("tracer", SignalSink::TracerColor(id)),
    ]
}

/// A short unit label for a sensor source's readout — the catalog dimension's
/// SI symbol, so the label can never drift from the value (this used to be a
/// hard-coded table and still said "px/s"/"px" after the SI flip).
pub fn unit(source: &SignalSource) -> &'static str {
    source.dimension().symbol()
}

/// A sensor readout: a base-SI `value` in the unit the display preference
/// picks for `dimension`, at a fixed two places so a live row does not
/// jitter (`0.50 m`, `90.00°`). The joint inspector's limits use the same
/// preference, so an angle sensor and the limits it is read against agree
/// (brief 5-3).
pub fn readout(ctx: &egui::Context, dimension: Dimension, value: f32) -> String {
    let unit = crate::unit_fields::display(ctx, dimension);
    format!("{:.2}{}", unit.from_si(f64::from(value)), unit.suffix())
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
    sensor_rows(
        ui,
        id,
        body_sensors(id),
        index,
        physics,
        transforms,
        dt,
        bindings,
    );
    // Mass is a catalog row no signal source binds (it is not plotted), so
    // it reads here directly through the row's reader.
    if let Some(row) = quantity("mass") {
        ui.horizontal(|ui| {
            ui.label(row.label);
            let value = index.entity(id).and_then(|e| {
                let position = transforms.get(e).ok().map(|t| t.translation.truncate());
                row.read_body(&physics.body(e, position, dt))
            });
            match value {
                Some(mass) => ui.monospace(readout(ui.ctx(), row.dimension, mass)),
                None => widgets::empty_state(ui, "—"),
            };
        });
    }
}

/// Renders sensor ports as live read-only rows (name → value → a **plot**
/// toggle) — shared by the body and joint inspectors. `owner` names the plot
/// binding (`"{label}@{owner}"`); `dt` scales the impulse→force readout.
#[expect(
    clippy::too_many_arguments,
    reason = "the read_source inputs + row context"
)]
pub fn sensor_rows(
    ui: &mut egui::Ui,
    owner: StableId,
    sensors: impl IntoIterator<Item = (&'static str, SignalSource)>,
    index: &IdIndex,
    physics: &PhysicsQueries,
    transforms: &Query<&Transform, With<Body>>,
    dt: f32,
    bindings: &mut SignalBindings,
) {
    let id = owner;
    for (label, source) in sensors {
        ui.horizontal(|ui| {
            ui.label(label);
            match gradiance_signal::read_source(&source, index, physics, transforms, dt) {
                Some(value) => ui.monospace(readout(ui.ctx(), source.dimension(), value)),
                None => widgets::empty_state(ui, "—"),
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
                        name: format!("{label}@{}", crate::labels::short_id(&id)),
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
        let joint_units: Vec<&str> = joint_sensors(id).iter().map(|(_, s)| unit(s)).collect();
        assert_eq!(joint_units, ["m", "rad", "N"], "joint ports carry SI units");
    }

    #[test]
    fn every_port_is_a_catalog_row() {
        // The no-drift half on the UI side: a port reads its catalog row,
        // carries the row's label and unit, and names this id.
        let id = StableId::new();
        for (label, source) in body_sensors(id).into_iter().chain(joint_sensors(id)) {
            let (row, subjects) = source.reading().expect("a port reads a catalog row");
            assert_eq!(row.label, label);
            assert_eq!(unit(&source), row.dimension.symbol());
            assert_eq!(subjects, Subjects::One(id));
        }
        // Every body row a source can bind is a port (mass is script-only).
        let names: Vec<&str> = body_sensors(id)
            .iter()
            .filter_map(|(_, s)| s.quantity_name())
            .collect();
        assert_eq!(
            names,
            [
                "speed",
                "spin",
                "height",
                "pos-x",
                "contact-force",
                "contact-count",
                "kinetic-energy",
                "momentum",
                "angular-momentum"
            ]
        );
    }
}
