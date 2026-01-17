use crate::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, egui};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Egui setup
        app.add_plugins(EguiPlugin::default());

        // Placeholder system for sidebar
        app.add_systems(Update, sidebar_ui);
    }
}

fn sidebar_ui(mut contexts: EguiContexts) {
    // In Bevy 0.18 / Egui 0.39, ctx_mut might return a Result.
    // We handle it gracefully.
    if let Ok(ctx) = contexts.ctx_mut() {
        egui::SidePanel::left("tools_panel").show(ctx, |ui| {
            ui.heading("Gradiance Tools");
            ui.label("Select a tool from the list below.");
            // Tool buttons will go here
        });
    }
}
