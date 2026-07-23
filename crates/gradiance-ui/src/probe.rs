//! Live probes: the read facade surfaced in the UI.
//!
//! Hover a body (with hover-probe on) for a transient readout, or pin
//! bodies from the context menu into the *Probes* window for a persistent
//! one. A probe is a pure *read* of `physics::queries` and `Transform` —
//! no mutation, no persistence (pins are workstation state, entities
//! resolved by `StableId` so undo/redo cycles keep them valid).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;
use gradiance_interaction::PointerOverUi;
use gradiance_interaction::cursor::CursorWorldPos;
use gradiance_interaction::tools::topmost_body_at;
use gradiance_physics::queries::PhysicsQueries;
// `Time` stays fully-qualified at its one use site — bevy's `Time` (in the
// prelude) shares the name.
use gradiance_units::{AngularVelocity, Energy, Force, Mass, Momentum, Velocity, Velocity2};

/// Probe window state: pinned bodies plus the hover-probe toggle.
#[derive(Resource, Default)]
pub struct ProbePanel {
    open: bool,
    /// Hover readout enabled (independent of the window being open).
    pub hover: bool,
    /// Pinned bodies, in pin order.
    pub pinned: Vec<StableId>,
}

impl ProbePanel {
    /// Whether the window is shown (read by the transport toggle).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the window's visibility.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Pins `id` (idempotent) and opens the window so the pin is visible.
    pub fn pin(&mut self, id: StableId) {
        if !self.pinned.contains(&id) {
            self.pinned.push(id);
        }
        self.open = true;
    }
}

/// One body's live readout, formatted. Pure (unit-tested below).
pub fn probe_summary(
    pos: Vec2,
    velocity: Option<(Velocity2, AngularVelocity)>,
    mass: Option<Mass>,
    angular_inertia: Option<f32>,
    contact_force: Force,
    sleeping: bool,
) -> String {
    use std::fmt::Write;
    // Unit labels come from the quantity types, so they cannot drift from the
    // values (before P2·types this line read a hard-coded "px/s" — stale after
    // the SI flip).
    let mut out = format!("pos ({:.1}, {:.1})", pos.x, pos.y);
    if let Some((v, omega)) = velocity {
        let _ = write!(
            out,
            "\nv {:.1} {}  ω {:.2} {}",
            v.magnitude().value(),
            Velocity::UNIT,
            omega.value(),
            AngularVelocity::UNIT,
        );
    }
    if let Some(m) = mass {
        let _ = write!(out, "\nmass {:.1} {}", m.value(), Mass::UNIT);
    }
    // Total kinetic energy ½mv² + ½Iω² — a derived quantity assembled from the
    // typed facade reads (mass, velocity, angular inertia), shown in joules.
    if let (Some((v, omega)), Some(m)) = (velocity, mass) {
        let translational = 0.5 * m.value() * v.magnitude().value().powi(2);
        let rotational = angular_inertia.map_or(0.0, |i| 0.5 * i * omega.value().powi(2));
        let _ = write!(
            out,
            "\nKE {:.3} {}",
            translational + rotational,
            Energy::UNIT
        );
        // Linear momentum |p| = m·|v|, the other conserved quantity.
        let p = m.value() * v.magnitude().value();
        let _ = write!(out, "\np {p:.2} {}", Momentum::UNIT);
    }
    let _ = write!(
        out,
        "\ncontact {:.0} {}",
        contact_force.value(),
        Force::UNIT
    );
    if sleeping {
        out.push_str("\nsleeping");
    }
    out
}

/// Renders the Probes window and the hover readout.
#[expect(clippy::too_many_arguments)] // read-only feeds, one per fact
pub fn probe_panel(
    mut contexts: EguiContexts,
    mut panel: ResMut<ProbePanel>,
    index: Res<IdIndex>,
    transforms: Query<&Transform, With<Body>>,
    bodies: Query<(&ShapeDef, &DepthBand), With<Body>>,
    physics: PhysicsQueries,
    fixed: Res<Time<Fixed>>,
    cursor: Res<CursorWorldPos>,
    over_ui: Res<PointerOverUi>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let dt = fixed.timestep().as_secs_f32().max(1e-6);
    let summary = |entity: Entity| -> Option<String> {
        let pos = transforms.get(entity).ok()?.translation.truncate();
        Some(probe_summary(
            pos,
            physics.velocity_of(entity),
            physics.mass_of(entity),
            physics.angular_inertia_of(entity),
            // Impulse ÷ dt is the contact force (typed relation).
            physics.net_contact_impulse(entity).magnitude() / gradiance_units::Time::seconds(dt),
            physics.is_sleeping(entity),
        ))
    };

    if panel.open {
        let mut open = true;
        egui::Window::new("Probes")
            .open(&mut open)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.checkbox(&mut panel.hover, "hover readout")
                    .on_hover_text("show live physics for the body under the cursor");
                if panel.pinned.is_empty() {
                    ui.label("Pin bodies from the right-click menu.");
                }
                let mut unpin = None;
                for (i, id) in panel.pinned.iter().enumerate() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.monospace(format!("#{}", &id.0.to_string()[..8]));
                        if ui.small_button("✖").clicked() {
                            unpin = Some(i);
                        }
                    });
                    match index.entity(*id).and_then(&summary) {
                        Some(text) => {
                            ui.label(text);
                        }
                        None => {
                            ui.weak("(not in scene)");
                        }
                    }
                }
                if let Some(i) = unpin {
                    panel.pinned.remove(i);
                }
            });
        panel.open = open;
    }

    // The hover readout: a small floating readout beside the cursor for
    // the topmost body under it (never over egui itself).
    if panel.hover
        && !over_ui.0
        && let Some(world_pos) = cursor.0
        && let Some(entity) = topmost_body_at(world_pos, &physics, &bodies)
        && let Some(text) = summary(entity)
        && let Some(pointer) = ctx.pointer_latest_pos()
    {
        egui::Area::new(egui::Id::new("hover-probe"))
            .fixed_pos(pointer + egui::vec2(16.0, 16.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new(text).monospace().size(10.0));
                });
            });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_summary_reports_the_read_facade_facts() {
        let text = probe_summary(
            Vec2::new(10.0, -2.5),
            Some((Velocity2::new(Vec2::new(3.0, 4.0)), AngularVelocity(1.5))),
            Some(Mass(400.0)),
            Some(8.0),
            Force(980.0),
            true,
        );
        assert!(text.contains("pos (10.0, -2.5)"));
        // Units read off the quantity type — SI now, and can't drift.
        assert!(text.contains("v 5.0 m/s"), "{text}");
        assert!(text.contains("ω 1.50 rad/s"));
        assert!(text.contains("mass 400.0 kg"));
        // ½mv² + ½Iω² = ½·400·5² + ½·8·1.5² = 5000 + 9 = 5009 J.
        assert!(text.contains("KE 5009.000 J"), "{text}");
        // |p| = m·|v| = 400·5 = 2000 kg·m/s.
        assert!(text.contains("p 2000.00 kg·m/s"), "{text}");
        assert!(text.contains("contact 980 N"));
        assert!(text.contains("sleeping"));

        // A static body (no velocity) still probes.
        let text = probe_summary(Vec2::ZERO, None, None, None, Force(0.0), false);
        assert!(text.contains("pos (0.0, 0.0)"));
        assert!(!text.contains("sleeping"));
    }
}
