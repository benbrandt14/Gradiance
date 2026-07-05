//! Tool palette + transport (play/pause, undo/redo, scale frame).

use crate::command::intent::{RedoIntent, UndoIntent};
use crate::core::states::{GameState, ToolState};
use crate::interaction::tools::handles::ScaleFrame;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

const TOOLS: [(ToolState, &str, &str); 10] = [
    (ToolState::Select, "Select", "S"),
    (ToolState::Drag, "Drag", "D"),
    (ToolState::Box, "Box", "B"),
    (ToolState::Circle, "Circle", "C"),
    (ToolState::Polygon, "Polygon", "P"),
    (ToolState::Hinge, "Hinge", "H"),
    (ToolState::Weld, "Weld", "W"),
    (ToolState::Slider, "Slider", "R"),
    (ToolState::Ground, "Ground", "G"),
    (ToolState::Cut, "Cut", "K"),
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
                if ui.button("⚙ Settings").clicked() {
                    settings.open = !settings.open;
                }
            });
        });

    egui::Window::new("Tools")
        .resizable(false)
        .anchor(egui::Align2::LEFT_CENTER, [8.0, 0.0])
        .show(ctx, |ui| {
            for (state, name, key) in TOOLS {
                let selected = *tool.get() == state;
                if ui
                    .selectable_label(selected, format!("{name} ({key})"))
                    .clicked()
                {
                    next_tool.set(state);
                }
            }
        });
    Ok(())
}
