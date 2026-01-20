//! Diagnostic tools for visualizing FPS, entity count, and system info.

use crate::prelude::*;
use bevy::diagnostic::{
    DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
    SystemInformationDiagnosticsPlugin,
};
use bevy_egui::{egui, EguiContexts};
use bevy_rapier2d::render::DebugRenderContext;

/// State for the diagnostics display.
#[derive(Default, Resource)]
pub struct DiagnosticsState {
    /// Whether to show the FPS.
    pub show_fps: bool,
    /// Whether to show entity count and system info.
    pub show_stats: bool,
}

/// Plugin for the diagnostics system.
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiagnosticsState>()
            .add_plugins((
                FrameTimeDiagnosticsPlugin::default(),
                EntityCountDiagnosticsPlugin::default(),
                SystemInformationDiagnosticsPlugin::default(),
            ))
            .add_systems(Update, (toggle_diagnostics, diagnostics_ui));
    }
}

fn toggle_diagnostics(
    mut diagnostics_state: ResMut<DiagnosticsState>,
    mut debug_render_context: Option<ResMut<DebugRenderContext>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if keys.just_pressed(KeyCode::F1) {
        if let Some(ref mut ctx) = debug_render_context {
            ctx.enabled = !ctx.enabled;
        }
    }
    if keys.just_pressed(KeyCode::F2) {
        diagnostics_state.show_fps = !diagnostics_state.show_fps;
    }
    if keys.just_pressed(KeyCode::F3) {
        diagnostics_state.show_stats = !diagnostics_state.show_stats;
    }
}

fn diagnostics_ui(
    diagnostics_state: Res<DiagnosticsState>,
    diagnostics: Res<DiagnosticsStore>,
    mut contexts: EguiContexts,
) {
    if !diagnostics_state.show_fps && !diagnostics_state.show_stats {
        return;
    }

    let ctx = contexts.ctx_mut();
    egui::Window::new("Diagnostics")
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-10.0, 10.0))
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .auto_sized()
        .show(ctx, |ui| {
            if diagnostics_state.show_fps {
                if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
                    if let Some(value) = fps.smoothed() {
                        ui.label(format!("FPS: {:.1}", value));
                    }
                }
                if let Some(frame_time) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME) {
                    if let Some(value) = frame_time.smoothed() {
                        ui.label(format!("Frame Time: {:.2} ms", value));
                    }
                }
            }

            if diagnostics_state.show_stats {
                if diagnostics_state.show_fps {
                    ui.separator();
                }

                if let Some(count) = diagnostics.get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT) {
                    if let Some(value) = count.values().last() {
                         ui.label(format!("Entities: {:.0}", value));
                    }
                }

                if let Some(cpu) = diagnostics.get(&SystemInformationDiagnosticsPlugin::CPU_USAGE) {
                    if let Some(value) = cpu.values().last() {
                         ui.label(format!("CPU: {:.1}%", value));
                    }
                }
                if let Some(mem) = diagnostics.get(&SystemInformationDiagnosticsPlugin::MEM_USAGE) {
                    if let Some(value) = mem.values().last() {
                         ui.label(format!("MEM: {:.1}%", value));
                    }
                }
            }
        });
}
