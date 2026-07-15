//! Tool palette + transport (play/pause, undo/redo, scale frame).

use crate::command::intent::{RedoIntent, UndoIntent};
use crate::core::states::{GameState, ToolState};
use crate::interaction::tools::handles::ScaleFrame;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

const TOOLS: [(ToolState, &str, &str); 12] = [
    (ToolState::Select, "Select", "S"),
    (ToolState::Drag, "Drag", "D"),
    (ToolState::Box, "Box", "B"),
    (ToolState::Circle, "Circle", "C"),
    (ToolState::Polygon, "Polygon", "P"),
    (ToolState::Hinge, "Hinge", "H"),
    (ToolState::Weld, "Weld", "W"),
    (ToolState::Slider, "Prismatic", "R"),
    (ToolState::Strut, "Strut", "T"),
    (ToolState::Ground, "Ground", "G"),
    (ToolState::Cut, "Cut", "K"),
    (ToolState::Tracer, "Tracer", "Y"),
];

/// Left tool palette and top transport strip.
pub fn toolbar(
    mut contexts: EguiContexts,
    tool: Res<State<ToolState>>,
    mut next_tool: ResMut<NextState<ToolState>>,
    game: Res<State<GameState>>,
    mut next_game: ResMut<NextState<GameState>>,
    mut frame: ResMut<ScaleFrame>,
    mut undo: MessageWriter<UndoIntent>,
    mut redo: MessageWriter<RedoIntent>,
    mut settings: ResMut<crate::ui::settings::SettingsWindow>,
    mut plot: ResMut<crate::ui::plot::PlotPanel>,
    mut probe: ResMut<crate::ui::probe::ProbePanel>,
    mut signals: ResMut<crate::ui::signals::SignalsPanel>,
    mut inspector: ResMut<crate::ui::inspector::InspectorPanel>,
    mut console: ResMut<crate::ui::console::ScriptConsole>,
    mut debug: ResMut<crate::domain::settings::DebugSettings>,
    mut rig: ResMut<crate::interaction::camera::CameraRig>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    egui::Window::new("transport")
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let playing = *game.get() == GameState::Playing;
                if ui
                    .button(if playing { "⏸ Pause" } else { "▶ Play" })
                    .clicked()
                {
                    next_game.set(if playing {
                        GameState::Paused
                    } else {
                        GameState::Playing
                    });
                }
                ui.separator();
                if ui.button("⟲ Undo").clicked() {
                    undo.write(UndoIntent);
                }
                if ui.button("⟳ Redo").clicked() {
                    redo.write(RedoIntent);
                }
                ui.separator();
                let label = match *frame {
                    ScaleFrame::Global => "Frame: Global (F)",
                    ScaleFrame::Local => "Frame: Local (F)",
                };
                if ui.button(label).clicked() {
                    *frame = match *frame {
                        ScaleFrame::Global => ScaleFrame::Local,
                        ScaleFrame::Local => ScaleFrame::Global,
                    };
                }
                ui.separator();
                // Re-home the orbited view back to the straight-on 2D
                // view (also bound to Home). Enabled only when tilted.
                if ui
                    .add_enabled(!rig.is_flat(), egui::Button::new("⌂ 2D view"))
                    .on_hover_text("return the camera to the flat 2D view (Home)")
                    .clicked()
                {
                    rig.glide_home();
                }
                ui.separator();
                panel_toggles(
                    ui,
                    &mut inspector,
                    &mut plot,
                    &mut probe,
                    &mut signals,
                    &mut console,
                    &mut debug,
                );
                ui.separator();
                if ui.button("⚙ Settings").clicked() {
                    settings.open = !settings.open;
                }
            });
        });

    egui::Window::new("Tools")
        .resizable(false)
        .anchor(egui::Align2::LEFT_CENTER, [8.0, 0.0])
        .show(ctx, |ui| {
            if let Some(state) = tools_palette_ui(ui, *tool.get()) {
                next_tool.set(state);
            }
        });
    Ok(())
}

/// Toggles for the hotkey-only panels, so they're discoverable without
/// knowing the `\` / backquote shortcuts. Each button reflects its panel's
/// open state. (The field overlay lives with the debug toggles, but it's
/// the main way to *see* attraction/repulsion — surfaced here too.)
fn panel_toggles(
    ui: &mut egui::Ui,
    inspector: &mut crate::ui::inspector::InspectorPanel,
    plot: &mut crate::ui::plot::PlotPanel,
    probe: &mut crate::ui::probe::ProbePanel,
    signals: &mut crate::ui::signals::SignalsPanel,
    console: &mut crate::ui::console::ScriptConsole,
    debug: &mut crate::domain::settings::DebugSettings,
) {
    if ui
        .selectable_label(inspector.open, "Properties")
        .on_hover_text("properties pop-out (also in the right-click menu)")
        .clicked()
    {
        inspector.open = !inspector.open;
    }
    if ui
        .selectable_label(plot.is_open(), "Plot")
        .on_hover_text("live plot of the selected body/joint (\\)")
        .clicked()
    {
        plot.toggle();
    }
    if ui
        .selectable_label(probe.is_open(), "Probe")
        .on_hover_text("live physics readouts: pinned bodies + hover")
        .clicked()
    {
        probe.toggle();
    }
    if ui
        .selectable_label(signals.is_open(), "Signals")
        .on_hover_text("wire scene attributes to colors and plots (signal dataflow)")
        .clicked()
    {
        signals.toggle();
    }
    if ui
        .selectable_label(console.is_open(), "λ Script")
        .on_hover_text("scripting console / REPL (`)")
        .clicked()
    {
        console.toggle();
    }
    if ui
        .selectable_label(debug.show_fields, "⇢ Fields")
        .on_hover_text("vector plot of the superposed field (also in Settings ▸ Debug)")
        .clicked()
    {
        debug.show_fields = !debug.show_fields;
    }
}

/// The tool-palette buttons: highlights `current`, returns a clicked tool.
/// Host-agnostic (pure `Ui` in, choice out) so `tests/it/ui_panels.rs` can
/// exercise it under `egui_kittest`.
pub fn tools_palette_ui(ui: &mut egui::Ui, current: ToolState) -> Option<ToolState> {
    let mut clicked = None;
    for (state, name, key) in TOOLS {
        let selected = current == state;
        if ui
            .selectable_label(selected, format!("{name} ({key})"))
            .clicked()
        {
            clicked = Some(state);
        }
    }
    clicked
}
