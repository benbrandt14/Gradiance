//! Game Settings Menu.
//!
//! Provides a UI for configuring physics and rendering settings.

use crate::prelude::*;
use crate::visuals::RenderSettings;
use bevy_egui::{EguiContexts, egui};

/// State of the Game Menu.
#[derive(Resource, Default)]
pub struct GameMenuState {
    /// Whether the menu is currently visible.
    pub is_open: bool,
    /// The currently selected tab.
    pub current_tab: MenuTab,
}

/// Available tabs in the Game Menu.
#[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum MenuTab {
    /// Physics configuration tab.
    #[default]
    Physics,
    /// Rendering configuration tab.
    Rendering,
}

/// Plugin for the Game Menu.
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMenuState>()
            .add_systems(Update, menu_ui);
    }
}

fn menu_ui(
    mut contexts: EguiContexts,
    mut menu_state: ResMut<GameMenuState>,
    // Physics Resources
    mut rapier_config: Query<&mut RapierConfiguration>,
    mut rapier_context: Query<&mut RapierContext>,
    mut debug_render: ResMut<DebugRenderContext>,
    mut time: ResMut<Time<Virtual>>,
    mut fixed_time: ResMut<Time<Fixed>>,
    // Rendering Resources
    mut render_settings: ResMut<RenderSettings>,
) {
    if !menu_state.is_open {
        return;
    }

    let ctx = contexts.ctx_mut();

    let mut is_open = menu_state.is_open;
    egui::Window::new("Game Settings")
        .open(&mut is_open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut menu_state.current_tab, MenuTab::Physics, "Physics");
                ui.selectable_value(&mut menu_state.current_tab, MenuTab::Rendering, "Rendering");
            });
            ui.separator();

            match menu_state.current_tab {
                MenuTab::Physics => {
                    physics_tab(
                        ui,
                        &mut rapier_config,
                        &mut rapier_context,
                        &mut debug_render,
                        &mut time,
                        &mut fixed_time,
                    );
                }
                MenuTab::Rendering => {
                    rendering_tab(ui, &mut render_settings);
                }
            }
        });
    menu_state.is_open = is_open;
}

fn physics_tab(
    ui: &mut egui::Ui,
    rapier_config: &mut Query<&mut RapierConfiguration>,
    rapier_context: &mut Query<&mut RapierContext>,
    debug_render: &mut ResMut<DebugRenderContext>,
    time: &mut ResMut<Time<Virtual>>,
    fixed_time: &mut ResMut<Time<Fixed>>,
) {
    ui.heading("Physics Configuration");

    // Gravity
    ui.label("Gravity:");
    // In case there are multiple physics worlds (unlikely but possible via query)
    for mut config in rapier_config.iter_mut() {
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut config.gravity.x)
                    .speed(10.0)
                    .prefix("X: "),
            );
            ui.add(
                egui::DragValue::new(&mut config.gravity.y)
                    .speed(10.0)
                    .prefix("Y: "),
            );
        });

        ui.separator();

        // Use collapsible to hide advanced settings
        ui.collapsing("Advanced Solver Settings", |ui| {
            for mut context in rapier_context.iter_mut() {
                ui.label("Solver Iterations:");
                // Use drag value for usize
                let mut iterations = context.integration_parameters.num_solver_iterations.get();
                if ui
                    .add(
                        egui::DragValue::new(&mut iterations)
                            .range(1..=50)
                            .speed(1.0),
                    )
                    .changed()
                {
                    context.integration_parameters.num_solver_iterations =
                        std::num::NonZeroUsize::new(iterations)
                            .unwrap_or(std::num::NonZeroUsize::new(1).unwrap());
                }

                ui.label("Friction Iterations:");
                let mut f_iterations = context
                    .integration_parameters
                    .num_additional_friction_iterations;
                if ui
                    .add(
                        egui::DragValue::new(&mut f_iterations)
                            .range(0..=50)
                            .speed(1.0),
                    )
                    .changed()
                {
                    context
                        .integration_parameters
                        .num_additional_friction_iterations = f_iterations;
                }

                ui.label("Max CCD Substeps:");
                let mut ccd_substeps = context.integration_parameters.max_ccd_substeps;
                if ui
                    .add(
                        egui::DragValue::new(&mut ccd_substeps)
                            .range(0..=10)
                            .speed(1.0),
                    )
                    .changed()
                {
                    context.integration_parameters.max_ccd_substeps = ccd_substeps;
                }
            }
        });
    }

    ui.separator();

    // Fixed Timestep
    ui.label("Physics Timestep:");
    let mut hz = 1.0 / fixed_time.timestep().as_secs_f64();
    if ui
        .add(
            egui::DragValue::new(&mut hz)
                .speed(1.0)
                .prefix("Hz: ")
                .range(1.0..=240.0),
        )
        .changed()
        && hz > 0.0
    {
        fixed_time.set_timestep_hz(hz);
    }

    ui.separator();

    // Time Scale
    ui.label("Time Scale:");
    let mut speed = time.relative_speed() as f64; // DragValue prefers f64
    if ui
        .add(
            egui::Slider::new(&mut speed, 0.0..=5.0)
                .text("Speed")
                .logarithmic(false),
        )
        .changed()
    {
        time.set_relative_speed(speed as f32);
    }

    ui.horizontal(|ui| {
        if ui.button("0.1x").clicked() {
            time.set_relative_speed(0.1);
        }
        if ui.button("0.5x").clicked() {
            time.set_relative_speed(0.5);
        }
        if ui.button("1.0x").clicked() {
            time.set_relative_speed(1.0);
        }
        if ui.button("2.0x").clicked() {
            time.set_relative_speed(2.0);
        }
    });

    ui.separator();

    // Debug Render
    ui.checkbox(&mut debug_render.enabled, "Show Physics Debug Info");
}

fn rendering_tab(ui: &mut egui::Ui, settings: &mut ResMut<RenderSettings>) {
    ui.heading("Rendering Configuration");

    ui.group(|ui| {
        ui.label("Shading Mode");
        ui.radio_value(&mut settings.toon_mode, false, "Standard (PBR)");
        ui.radio_value(&mut settings.toon_mode, true, "Toon Shading");
    });

    if settings.toon_mode {
        ui.separator();
        ui.label("Toon Settings");
        ui.indent("toon_settings", |ui| {
            ui.add(egui::Slider::new(&mut settings.toon_steps, 1..=10).text("Bands/Steps"));

            ui.horizontal(|ui| {
                ui.label("Sun Color:");
                let mut sun_color = settings.sun_color.to_srgba().to_f32_array();
                if ui
                    .color_edit_button_rgba_unmultiplied(&mut sun_color)
                    .changed()
                {
                    settings.sun_color =
                        Color::srgba(sun_color[0], sun_color[1], sun_color[2], sun_color[3]);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Ambient Color:");
                let mut amb_color = settings.ambient_color.to_srgba().to_f32_array();
                if ui
                    .color_edit_button_rgba_unmultiplied(&mut amb_color)
                    .changed()
                {
                    settings.ambient_color =
                        Color::srgba(amb_color[0], amb_color[1], amb_color[2], amb_color[3]);
                }
            });
        });
    }

    ui.separator();

    ui.checkbox(&mut settings.shadows_enabled, "Shadows Enabled");

    ui.separator();

    ui.checkbox(&mut settings.bloom_enabled, "Bloom Enabled");
    if settings.bloom_enabled {
        ui.indent("bloom_settings", |ui| {
            ui.add(egui::Slider::new(&mut settings.bloom_intensity, 0.0..=1.0).text("Intensity"));
        });
    }

    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Clear Color (Background):");
        let mut color = settings.clear_color.to_srgba().to_f32_array();
        if ui.color_edit_button_rgba_unmultiplied(&mut color).changed() {
            settings.clear_color = Color::srgba(color[0], color[1], color[2], color[3]);
        }
    });
}
