//! Main UI panels (Sidebar and Top bar).
//!
//! Handles the layout and logic for the tool selection sidebar and the top control bar
//! (Play/Pause, Grid settings, Time control).

use crate::input::ToolState;
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use crate::ui::icons::GameIcons;
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
    game_icons: Res<GameIcons>,
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

    let tools = [
        (
            ToolState::Select,
            contexts.add_image(game_icons.select.clone_weak()),
            "Select",
        ),
        (
            ToolState::Drag,
            contexts.add_image(game_icons.drag.clone_weak()),
            "Drag",
        ),
        (
            ToolState::Box,
            contexts.add_image(game_icons.box_tool.clone_weak()),
            "Box",
        ),
        (
            ToolState::Circle,
            contexts.add_image(game_icons.circle_tool.clone_weak()),
            "Circle",
        ),
        (
            ToolState::Polygon,
            contexts.add_image(game_icons.polygon_tool.clone_weak()),
            "Polygon",
        ),
        (
            ToolState::RevoluteJoint,
            contexts.add_image(game_icons.revolute_joint.clone_weak()),
            "Axle",
        ),
        (
            ToolState::Weld,
            contexts.add_image(game_icons.weld.clone_weak()),
            "Fix",
        ),
        (
            ToolState::Ground,
            contexts.add_image(game_icons.ground.clone_weak()),
            "Ground",
        ),
    ];

    let ctx = contexts.ctx_mut();

    egui::SidePanel::left("tools_panel").show(ctx, |ui| {
        ui.heading("Tools");
        ui.separator();

        for (state, texture_id, name) in tools {
            let is_selected = *current_tool_state.get() == state;
            if ui
                .add(
                    egui::ImageButton::new((texture_id, egui::Vec2::new(32.0, 32.0)))
                        .selected(is_selected),
                )
                .on_hover_text(name)
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
    game_icons: Res<GameIcons>,
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

    let play_icon = contexts.add_image(game_icons.play.clone_weak());
    let pause_icon = contexts.add_image(game_icons.pause.clone_weak());

    let ctx = contexts.ctx_mut();

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let (icon, tooltip) = if virtual_time.is_paused() {
                (play_icon, "Play")
            } else {
                (pause_icon, "Pause")
            };

            if ui
                .add(egui::ImageButton::new((icon, egui::Vec2::new(24.0, 24.0))))
                .on_hover_text(tooltip)
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
                ui.checkbox(&mut grid_settings.auto_spacing, "Auto");
                if !grid_settings.auto_spacing {
                    ui.add(
                        egui::DragValue::new(&mut grid_settings.spacing)
                            .speed(0.1)
                            .range(0.1..=100.0)
                            .prefix("Spc: "),
                    );
                }

                ui.separator();

                // Toggle Grid Snap
                ui.checkbox(&mut grid_settings.snap_to_grid, "Snap Grid");

                // Toggle Object Snap
                ui.checkbox(&mut grid_settings.snap_to_objects, "Snap Obj");

                if grid_settings.snap_to_objects {
                     ui.menu_button("...", |ui| {
                        ui.label("Object Snap Options");
                        ui.checkbox(&mut grid_settings.snap_to_object_centers, "Centers");
                        ui.checkbox(&mut grid_settings.snap_to_object_corners, "Corners");
                        ui.checkbox(&mut grid_settings.snap_to_object_edges, "Edges");
                        ui.checkbox(&mut grid_settings.snap_to_object_midpoints, "Midpoints");
                        ui.separator();
                        ui.add(egui::DragValue::new(&mut grid_settings.snap_distance).speed(0.1).range(0.01..=100.0).prefix("Dist: "));
                    });
                }
            }
        });
    });
}
