use crate::prelude::*;
use crate::input::ToolState;
use bevy_egui::{EguiPlugin, EguiContexts, egui};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Egui setup
        app.add_plugins(EguiPlugin::default());

        // Placeholder system for sidebar
        app.add_systems(Update, sidebar_ui);
    }
}

fn sidebar_ui(
    mut contexts: EguiContexts,
    current_tool: Res<State<ToolState>>,
    mut next_tool: ResMut<NextState<ToolState>>,
) {
    // In Bevy 0.18 / Egui 0.39, ctx_mut might return a Result.
    // We handle it gracefully.
    if let Ok(ctx) = contexts.ctx_mut() {
        egui::SidePanel::left("tools_panel").show(ctx, |ui| {
            ui.heading("Gradiance Tools");
            ui.label("Select a tool:");
            ui.separator();

            if ui.selectable_label(*current_tool.get() == ToolState::Select, "Select").clicked() {
                next_tool.set(ToolState::Select);
            }
            if ui.selectable_label(*current_tool.get() == ToolState::Box, "Box").clicked() {
                next_tool.set(ToolState::Box);
            }
            if ui.selectable_label(*current_tool.get() == ToolState::Circle, "Circle").clicked() {
                next_tool.set(ToolState::Circle);
            }
        });
    }
}
