//! UI Panels configuration (Sidebar, Bottom Bar).

use crate::input::ToolState;
use crate::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Plugin for the main UI panels.
pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (sidebar_ui, bottom_bar_ui));
    }
}

fn sidebar_ui(
    mut contexts: EguiContexts,
    current_tool: Res<State<ToolState>>,
    mut next_tool: ResMut<NextState<ToolState>>,
) {
    let ctx = contexts.ctx_mut();

    egui::SidePanel::left("tools_panel")
        .resizable(false)
        .default_width(50.0)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Tools");
                ui.separator();

                let tools = [
                    ("Select", ToolState::Select),
                    ("Drag", ToolState::Drag),
                    ("Box", ToolState::Box),
                    ("Circle", ToolState::Circle),
                    ("Poly", ToolState::Polygon),
                    ("Axle", ToolState::RevoluteJoint),
                    ("Weld", ToolState::Weld),
                ];

                for (name, state) in tools {
                    let is_selected = *current_tool.get() == state;
                    if ui.selectable_label(is_selected, name).clicked() {
                        next_tool.set(state);
                    }
                }
            });
        });
}

fn bottom_bar_ui(
    mut contexts: EguiContexts,
    mut time: ResMut<Time<Virtual>>,
) {
    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button(if time.is_paused() { "▶ Play" } else { "⏸ Pause" }).clicked() {
                if time.is_paused() {
                    time.unpause();
                } else {
                    time.pause();
                }
            }

            ui.label(format!("Time Scale: {:.1}x", time.relative_speed()));

            if ui.button("<<").clicked() {
                 let s = (time.relative_speed() - 0.1).max(0.0);
                 time.set_relative_speed(s);
            }
            if ui.button(">>").clicked() {
                 let s = time.relative_speed() + 0.1;
                 time.set_relative_speed(s);
            }
            if ui.button("Reset").clicked() {
                time.set_relative_speed(1.0);
            }
        });
    });
}
