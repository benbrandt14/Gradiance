//! Main UI panels (Sidebar and Top bar).
//!
//! Handles the layout and logic for the tool selection sidebar and the top control bar
//! (Play/Pause, Grid settings, Time control).

use crate::input::ToolState;
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

/// Plugin for the main UI panels.
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
    window_query: Query<&Window, With<PrimaryWindow>>,
) {
    let Some(window) = window_query.iter().next() else {
        return;
    };

    if window.width() <= 0.0
        || window.height() <= 0.0
        || window.physical_width() == 0
        || window.physical_height() == 0
    {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        _ => return,
    };

    if ctx.input(|i| i.screen_rect().is_none()) {
        return;
    }

    egui::SidePanel::left("tools_panel").show(ctx, |ui| {
        ui.heading("Tools");
        ui.separator();

        let tools = [
            ("Select", ToolState::Select),
            ("Drag", ToolState::Drag),
            ("Box", ToolState::Box),
            ("Circle", ToolState::Circle),
            ("Polygon", ToolState::Polygon),
            // Placeholder for other tools
            // ("Cut", ToolState::Cut),
            // ("Sketch", ToolState::Sketch),
        ];

        for (name, state) in tools {
            let is_selected = *current_tool_state.get() == state;
            if ui
                .add(egui::Button::new(name).selected(is_selected))
                .clicked()
            {
                next_tool_state.set(state);
            }
        }
    });
}

fn top_panel_ui(
    mut contexts: EguiContexts,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut grid_settings: ResMut<GridSettings>,
    window_query: Query<&Window, With<PrimaryWindow>>,
) {
    let Some(window) = window_query.iter().next() else {
        return;
    };

    if window.width() <= 0.0
        || window.height() <= 0.0
        || window.physical_width() == 0
        || window.physical_height() == 0
    {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        _ => return,
    };

    if ctx.input(|i| i.screen_rect().is_none()) {
        return;
    }

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui
                .button(if virtual_time.is_paused() {
                    "▶ Play"
                } else {
                    "⏸ Pause"
                })
                .clicked()
            {
                if virtual_time.is_paused() {
                    virtual_time.unpause();
                } else {
                    virtual_time.pause();
                }
            }

            ui.label(format!("Speed: {:.2}x", virtual_time.relative_speed()));

            ui.separator();
            ui.checkbox(&mut grid_settings.show, "Grid");
            if grid_settings.show {
                ui.checkbox(&mut grid_settings.snap, "Snap");
                ui.add(
                    egui::DragValue::new(&mut grid_settings.spacing)
                        .speed(0.1)
                        .range(0.1..=100.0)
                        .prefix("Spacing: "),
                );
            }
        });
    });
}
