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
    velocity: Option<(Vec2, f32)>,
    mass: Option<f32>,
    contact_force: f32,
    sleeping: bool,
) -> String {
    use std::fmt::Write;
    let mut out = format!("pos ({:.1}, {:.1})", pos.x, pos.y);
    if let Some((v, omega)) = velocity {
        let _ = write!(out, "\nv {:.1} px/s  ω {:.2} rad/s", v.length(), omega);
    }
    if let Some(m) = mass {
        let _ = write!(out, "\nmass {m:.1}");
    }
    let _ = write!(out, "\ncontact {contact_force:.0}");
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
            physics.net_contact_impulse(entity).length() / dt,
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
            Some((Vec2::new(3.0, 4.0), 1.5)),
            Some(400.0),
            980.0,
            true,
        );
        assert!(text.contains("pos (10.0, -2.5)"));
        assert!(text.contains("v 5.0 px/s"), "{text}");
        assert!(text.contains("ω 1.50 rad/s"));
        assert!(text.contains("mass 400.0"));
        assert!(text.contains("contact 980"));
        assert!(text.contains("sleeping"));

        // A static body (no velocity) still probes.
        let text = probe_summary(Vec2::ZERO, None, None, 0.0, false);
        assert!(text.contains("pos (0.0, 0.0)"));
        assert!(!text.contains("sleeping"));
    }
}
