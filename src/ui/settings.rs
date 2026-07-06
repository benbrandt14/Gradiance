//! Settings window: Algodoo-style tabs, reflection-driven contents.
//!
//! Each tab is `reflect_grid(resource)` — new settings fields appear in
//! the UI automatically. Enums (not reflect-derivable into widgets) get
//! explicit rows; that is the sanctioned escape hatch.

use crate::domain::settings::{GridSettings, GridSystem, RenderSettings, SimSettings, SnapConfig};
use crate::ui::reflect_grid::reflect_grid;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Which settings tab is open.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    /// Simulation tuning.
    #[default]
    Simulation,
    /// Grid & snapping.
    GridSnap,
    /// Rendering style.
    Rendering,
}

/// Settings window state.
#[derive(Resource, Default, Debug)]
pub struct SettingsWindow {
    /// Window visibility.
    pub open: bool,
    /// Active tab.
    pub tab: SettingsTab,
}

/// Renders the tabbed settings window.
pub fn settings_window(
    mut contexts: EguiContexts,
    mut window: ResMut<SettingsWindow>,
    mut sim: ResMut<SimSettings>,
    mut grid: ResMut<GridSettings>,
    mut snap: ResMut<SnapConfig>,
    mut render: ResMut<RenderSettings>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    if !window.open {
        return Ok(());
    }
    let mut open = window.open;
    egui::Window::new("Settings")
        .open(&mut open)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut window.tab, SettingsTab::Simulation, "Simulation");
                ui.selectable_value(&mut window.tab, SettingsTab::GridSnap, "Grid & Snap");
                ui.selectable_value(&mut window.tab, SettingsTab::Rendering, "Rendering");
            });
            ui.separator();
            match window.tab {
                SettingsTab::Simulation => {
                    reflect_grid(ui, egui::Id::new("sim"), sim.bypass_change_detection());
                    // Only flag the resource changed when egui actually
                    // edited something is overkill here; mark it touched.
                    sim.set_changed();
                }
                SettingsTab::GridSnap => {
                    ui.label(egui::RichText::new("Grid").strong());
                    // Enum escape hatch: explicit variant picker.
                    ui.horizontal(|ui| {
                        ui.label("system");
                        let current = grid.system;
                        egui::ComboBox::from_id_salt("grid-system")
                            .selected_text(format!("{current:?}"))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut grid.system,
                                    GridSystem::Cartesian,
                                    "Cartesian",
                                );
                                ui.selectable_value(
                                    &mut grid.system,
                                    GridSystem::Isometric,
                                    "Isometric",
                                );
                                ui.selectable_value(
                                    &mut grid.system,
                                    GridSystem::Polar {
                                        angular_divisions: 12,
                                    },
                                    "Polar",
                                );
                            });
                    });
                    reflect_grid(ui, egui::Id::new("grid"), grid.bypass_change_detection());
                    grid.set_changed();
                    ui.separator();
                    ui.label(egui::RichText::new("Snapping").strong());
                    reflect_grid(ui, egui::Id::new("snap"), snap.bypass_change_detection());
                    snap.set_changed();
                }
                SettingsTab::Rendering => {
                    reflect_grid(
                        ui,
                        egui::Id::new("render"),
                        render.bypass_change_detection(),
                    );
                    render.set_changed();
                }
            }
        });
    window.open = open;
    Ok(())
}
