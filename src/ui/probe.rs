//! Live **probe**: hover a body (in Select mode) to read its instantaneous
//! physics in a floating readout.
//!
//! The "probe" leg of read-total governance (`docs/script-lisp-decision.md`):
//! a pure read of the `physics::queries` facade surfaced in the UI — never a
//! mutation, never authored state. It reuses the exact front-most hit-test the
//! Select tool clicks with, so the readout always describes the body a click
//! would pick. The panel is a non-interactable tooltip-order `Area`, so it
//! never captures the pointer (no click-blocking, no hover flicker).

use crate::core::ids::StableId;
use crate::core::states::ToolState;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use crate::interaction::PointerOverUi;
use crate::interaction::cursor::CursorWorldPos;
use crate::interaction::tools::topmost_body_at;
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Draws a floating physics readout for the body under the cursor while the
/// Select tool is active. Pure read; runs only with rendering present.
pub fn hover_probe(
    mut contexts: EguiContexts,
    tool: Res<State<ToolState>>,
    over_ui: Res<PointerOverUi>,
    cursor: Res<CursorWorldPos>,
    bodies: Query<(&ShapeDef, &LayerMask32), With<Body>>,
    transforms: Query<&Transform>,
    ids: Query<&StableId>,
    physics: PhysicsQueries,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    // Select is the inspect tool; stay out of the drawing tools' and panels' way.
    if *tool.get() != ToolState::Select || over_ui.0 {
        return Ok(());
    }
    let (Some(world), Some(screen)) = (cursor.0, ctx.pointer_hover_pos()) else {
        return Ok(());
    };
    let Some(entity) = topmost_body_at(world, &physics, &bodies) else {
        return Ok(());
    };

    egui::Area::new(egui::Id::new("hover_probe"))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .fixed_pos(screen + egui::vec2(16.0, 16.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                probe_readout(ui, entity, &transforms, &ids, &physics);
            });
        });
    Ok(())
}

/// Renders one body's live physics as a compact monospace block.
fn probe_readout(
    ui: &mut egui::Ui,
    entity: Entity,
    transforms: &Query<&Transform>,
    ids: &Query<&StableId>,
    physics: &PhysicsQueries,
) {
    let id = ids
        .get(entity)
        .map_or_else(|_| "?".into(), |i| format!("{i:.8}"));
    ui.monospace(egui::RichText::new(format!("body {id}")).strong());

    if let Ok(transform) = transforms.get(entity) {
        let pose = PosRot::from_transform(transform);
        ui.monospace(format!("pos   ({:.0}, {:.0})", pose.pos.x, pose.pos.y));
        ui.monospace(format!("angle {:.0}°", pose.rot.to_degrees()));
    }

    match physics.velocity_of(entity) {
        Some((lin, ang)) => {
            ui.monospace(format!("speed {:.0} px/s", lin.length()));
            ui.monospace(format!("vel   ({:.0}, {:.0})", lin.x, lin.y));
            ui.monospace(format!("spin  {:.0} °/s", ang.to_degrees()));
            ui.monospace(if physics.is_sleeping(entity) {
                "asleep"
            } else {
                "awake"
            });
        }
        None => {
            ui.monospace("static");
        }
    }

    if let Some(mass) = physics.mass_of(entity).filter(|m| m.is_finite()) {
        ui.monospace(format!("mass  {mass:.1}"));
    }
}
