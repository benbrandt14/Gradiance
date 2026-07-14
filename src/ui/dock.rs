//! The right **dock**: one host panel shared by the Depth section and the
//! script console.
//!
//! egui side panels claim screen space from a root `Ui`, so stacked docks
//! must be drawn by one system — separate systems would each claim the
//! right edge independently and overlap. This host owns the panel; the
//! sections (`depth_panel::depth_section`, `console::console_section`)
//! stay self-contained renderers over their own state.

use crate::command::intent::PropertyEditIntent;
use crate::core::ids::StableId;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::depth::DepthBand;
use crate::interaction::selection::Selection;
use crate::script::bridge::{OperationRegistry, ScriptInputs, ScriptLog};
use crate::ui::console::{self, ScriptConsole};
use crate::ui::depth_panel::{self, DepthPanel};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Renders the dock when any section is open. The backquote key toggles
/// the console (unless something is capturing keyboard input).
#[expect(clippy::too_many_arguments)] // one dock host, grouped section reads
pub fn right_dock(
    mut contexts: EguiContexts,
    mut panel: ResMut<DepthPanel>,
    mut console: ResMut<ScriptConsole>,
    selection: Res<Selection>,
    ids: Query<&StableId, With<Body>>,
    bands: Query<&DepthBand, With<Body>>,
    appearances: Query<&Appearance, With<Body>>,
    mut edits: MessageWriter<PropertyEditIntent>,
    mut inputs: ResMut<ScriptInputs>,
    registry: Res<OperationRegistry>,
    log: Res<ScriptLog>,
    keys: Res<ButtonInput<KeyCode>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // Toggle with `` ` `` — but not while typing (so the key reaches editors).
    if keys.just_pressed(KeyCode::Backquote) && !ctx.egui_wants_keyboard_input() {
        console.toggle();
    }
    if !panel.open && !console.is_open() {
        return Ok(());
    }

    // Rows for the depth section: the selection projected to bars.
    let rows: Vec<(StableId, DepthBand, egui::Color32)> = selection
        .iter()
        .filter_map(|e| {
            let id = *ids.get(e).ok()?;
            let band = bands.get(e).ok()?.sanitized();
            let c = appearances
                .get(e)
                .map_or(crate::domain::appearance::Rgba::rgb(1.0, 1.0, 1.0), |a| {
                    a.fill
                });
            let color = egui::Color32::from_rgb(
                (c.r * 255.0) as u8,
                (c.g * 255.0) as u8,
                (c.b * 255.0) as u8,
            );
            Some((id, band, color))
        })
        .collect();

    // egui 0.35 panels dock inside a `Ui`; build the screen-root one.
    let mut root = egui::Ui::new(
        ctx.clone(),
        "right-dock".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::right("right-dock-panel")
        .resizable(true)
        .default_size(220.0)
        .show(&mut root, |ui| {
            if panel.open {
                // Leave the console its share when both sections are open.
                let max_height = if console.is_open() {
                    ui.available_height() * 0.45
                } else {
                    ui.available_height()
                };
                depth_panel::depth_section(ui, &mut panel, &rows, &mut edits, max_height);
            }
            if panel.open && console.is_open() {
                ui.separator();
            }
            if console.is_open() {
                console::console_section(ui, &mut console, &mut inputs, &registry, &log);
            }
        });
    Ok(())
}
