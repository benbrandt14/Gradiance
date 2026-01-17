use crate::prelude::*;
use crate::input::ToolState;
use bevy_egui::{EguiContexts, egui};

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (sidebar_ui, top_panel_ui));
    }
}

fn sidebar_ui(
    mut contexts: EguiContexts,
    mut next_tool_state: ResMut<NextState<ToolState>>,
    current_tool_state: Res<State<ToolState>>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        _ => return,
    };

    egui::SidePanel::left("tools_panel").show(ctx, |ui| {
        ui.heading("Tools");
        ui.separator();

        let tools = [
            ("Select", ToolState::Select),
            ("Box", ToolState::Box),
            ("Circle", ToolState::Circle),
            // Placeholder for other tools
            // ("Drag", ToolState::Drag),
            // ("Cut", ToolState::Cut),
            // ("Sketch", ToolState::Sketch),
        ];

        for (name, state) in tools {
            let is_selected = *current_tool_state.get() == state;
            if ui.add(egui::Button::new(name).selected(is_selected)).clicked() {
                next_tool_state.set(state);
            }
        }
    });
}

fn top_panel_ui(
    mut contexts: EguiContexts,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        _ => return,
    };

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button(if virtual_time.is_paused() { "▶ Play" } else { "⏸ Pause" }).clicked() {
                if virtual_time.is_paused() {
                    virtual_time.unpause();
                } else {
                    virtual_time.pause();
                }
            }

            ui.label(format!("Speed: {:.2}x", virtual_time.relative_speed()));
        });
    });
}
