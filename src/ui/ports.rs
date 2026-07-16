//! Object **ports**: the per-object sensor (read-only) and actuator
//! (writable) data the inspector surfaces and the node canvas wires.
//!
//! The Algodoo/Simulink framing the editor is moving toward: a body is not a
//! bespoke panel of unrelated widgets — it exposes named **ports**. A
//! **sensor** port is a read-only live quantity (height, speed, contact
//! force); an **actuator** port is a writable authored property (color,
//! friction, …). This module owns the *sensor* side — the live readouts —
//! reusing the one shared reader ([`read_quantity`](crate::signal::read_quantity))
//! so there is no third parallel "read a body quantity" implementation. The
//! actuator ports are the existing editable property widgets in
//! [`inspector`](crate::ui::inspector); the node canvas wires both (Stage B).

use crate::domain::Body;
use crate::domain::node::SensorQuantity;
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;
use bevy_egui::egui;

/// The sensor ports a rigid body exposes as live read-only quantities, in
/// display order. (Mass is rendered separately — it is derived, not a
/// [`SensorQuantity`].)
pub const BODY_SENSORS: [SensorQuantity; 6] = [
    SensorQuantity::Speed,
    SensorQuantity::Spin,
    SensorQuantity::PosX,
    SensorQuantity::PosY,
    SensorQuantity::ContactForce,
    SensorQuantity::ContactCount,
];

/// A short unit label for a sensor quantity's readout.
pub fn unit(quantity: SensorQuantity) -> &'static str {
    match quantity {
        SensorQuantity::Speed => "px/s",
        SensorQuantity::Spin => "rad/s",
        SensorQuantity::PosX | SensorQuantity::PosY | SensorQuantity::Height => "px",
        SensorQuantity::ContactForce => "N",
        SensorQuantity::ContactCount => "",
    }
}

/// Renders a body's sensor ports as live read-only rows (name → value). These
/// are the read-only half of "every datum is a port": the same quantities the
/// node canvas reads, shown in the inspector. `dt` scales the impulse→force
/// readout (matching the binding evaluator).
pub fn sensor_readouts(
    ui: &mut egui::Ui,
    entity: Entity,
    physics: &PhysicsQueries,
    transforms: &Query<&Transform, With<Body>>,
    dt: f32,
) {
    for quantity in BODY_SENSORS {
        ui.horizontal(|ui| {
            ui.label(quantity.label());
            match crate::signal::read_quantity(quantity, entity, physics, transforms, dt) {
                Some(value) => ui.monospace(format!("{value:.2} {}", unit(quantity))),
                None => ui.weak("—"),
            };
        });
    }
    ui.horizontal(|ui| {
        ui.label("mass");
        match physics.mass_of(entity) {
            Some(mass) => ui.monospace(format!("{mass:.2}")),
            None => ui.weak("—"),
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_sensor_ports_are_distinct_and_labelled() {
        let labels: Vec<&str> = BODY_SENSORS.iter().map(|q| q.label()).collect();
        let mut deduped = labels.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(deduped.len(), labels.len(), "duplicate sensor-port labels");
        // Every port has a (possibly empty) unit — an exhaustive match, so a
        // new SensorQuantity variant forces a unit here.
        for quantity in BODY_SENSORS {
            let _ = unit(quantity);
        }
    }
}
