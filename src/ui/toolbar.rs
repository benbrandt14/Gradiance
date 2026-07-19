//! Tool palette + transport (play/pause, undo/redo, scale frame).

use crate::command::intent::{RedoIntent, UndoIntent};
use crate::core::states::{GameState, ToolState};
use crate::interaction::tools::handles::ScaleFrame;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// The editor panels the transport strip toggles, bundled so the toolbar
/// system stays under Bevy's system-parameter limit.
#[derive(SystemParam)]
pub struct Panels<'w> {
    /// Settings window.
    pub settings: ResMut<'w, crate::ui::settings::SettingsWindow>,
    /// Properties inspector pop-out.
    pub inspector: ResMut<'w, crate::ui::inspector::InspectorPanel>,
    /// Live plot panel.
    pub plot: ResMut<'w, crate::ui::plot::PlotPanel>,
    /// Physics probe panel.
    pub probe: ResMut<'w, crate::ui::probe::ProbePanel>,
    /// Signals dock section.
    pub signals: ResMut<'w, crate::ui::signals::SignalsPanel>,
    /// Node-graph canvas.
    pub node_graph: ResMut<'w, crate::ui::node_graph::NodeGraph>,
    /// Object tree (outliner).
    pub outliner: ResMut<'w, crate::ui::outliner::ObjectTreePanel>,
    /// Scripting console.
    pub console: ResMut<'w, crate::ui::console::ScriptConsole>,
    /// Debug overlays (field vectors).
    pub debug: ResMut<'w, crate::domain::settings::DebugSettings>,
}

/// Tool-palette entries grouped into labeled sections (Blender-style): each
/// section is a heading plus its tools, in workflow order — pick/move, create a
/// shape, connect with a constraint, then modify. Kept as data so
/// `tools_palette_ui` stays a pure projection and `tests/it/ui_panels.rs` can
/// assert every tool still renders after a reflow.
const TOOL_GROUPS: &[(&str, &[(ToolState, &str, &str)])] = &[
    (
        "Select",
        &[
            (ToolState::Select, "Select", "S"),
            (ToolState::Drag, "Drag", "D"),
        ],
    ),
    (
        "Create",
        &[
            (ToolState::Box, "Box", "B"),
            (ToolState::Circle, "Circle", "C"),
            (ToolState::Polygon, "Polygon", "P"),
        ],
    ),
    (
        "Connect",
        &[
            (ToolState::Hinge, "Hinge", "H"),
            (ToolState::Slider, "Prismatic", "R"),
            (ToolState::Strut, "Strut", "T"),
            (ToolState::Weld, "Weld", "W"),
            (ToolState::Ground, "Ground", "G"),
        ],
    ),
    (
        "Modify",
        &[
            (ToolState::Cut, "Cut", "K"),
            (ToolState::Tracer, "Tracer", "Y"),
        ],
    ),
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
    mut panels: Panels,
    mut rig: ResMut<crate::interaction::camera::CameraRig>,
    panel_rects: Res<crate::ui::PanelRects>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    // Sit below the menu bar (it pushed its rect first this frame) so the
    // transport strip doesn't cover the File/Edit/View/Help menus.
    let viewport = ctx.viewport_rect();
    let top = panel_rects.top_inset(viewport) - viewport.top() + 4.0;
    egui::Window::new("transport")
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::LEFT_TOP, [8.0, top])
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
                panel_toggles(ui, &mut panels);
                ui.separator();
                if ui.button("⚙ Settings").clicked() {
                    panels.settings.open = !panels.settings.open;
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
fn panel_toggles(ui: &mut egui::Ui, panels: &mut Panels) {
    ui.label(egui::RichText::new("Panels").small().weak());
    if ui
        .selectable_label(panels.outliner.is_open(), "Outliner")
        .on_hover_text("object tree: every scene entity, grouped — click to select")
        .clicked()
    {
        panels.outliner.toggle();
    }
    if ui
        .selectable_label(panels.inspector.open, "Properties")
        .on_hover_text("properties pop-out (also in the right-click menu)")
        .clicked()
    {
        panels.inspector.open = !panels.inspector.open;
    }
    if ui
        .selectable_label(panels.plot.is_open(), "Plot")
        .on_hover_text("live plot of the selected body/joint (\\)")
        .clicked()
    {
        panels.plot.toggle();
    }
    if ui
        .selectable_label(panels.probe.is_open(), "Probe")
        .on_hover_text("live physics readouts: pinned bodies + hover")
        .clicked()
    {
        panels.probe.toggle();
    }
    if ui
        .selectable_label(panels.signals.is_open(), "Signals")
        .on_hover_text("wire scene attributes to colors and plots (signal dataflow)")
        .clicked()
    {
        panels.signals.toggle();
    }
    if ui
        .selectable_label(panels.node_graph.is_open(), "⬡ Graph")
        .on_hover_text("node-graph canvas: wire signals visually (drag output → actuator input)")
        .clicked()
    {
        panels.node_graph.toggle();
    }
    if ui
        .selectable_label(panels.console.is_open(), "λ Script")
        .on_hover_text("scripting console / REPL (`)")
        .clicked()
    {
        panels.console.toggle();
    }
    if ui
        .selectable_label(panels.debug.show_fields, "⇢ Fields")
        .on_hover_text("vector plot of the superposed field (also in Settings ▸ Debug)")
        .clicked()
    {
        panels.debug.show_fields = !panels.debug.show_fields;
    }
}

/// The tool-palette buttons, grouped into labeled sections: highlights
/// `current`, returns a clicked tool. Host-agnostic (pure `Ui` in, choice out)
/// so `tests/it/ui_panels.rs` can exercise it under `egui_kittest`. Each button
/// keeps its `"{name} ({key})"` label — the section headings are decoration.
pub fn tools_palette_ui(ui: &mut egui::Ui, current: ToolState) -> Option<ToolState> {
    let mut clicked = None;
    for (i, (group, tools)) in TOOL_GROUPS.iter().enumerate() {
        if i > 0 {
            ui.add_space(6.0);
        }
        ui.label(egui::RichText::new(*group).small().weak());
        for (state, name, key) in *tools {
            let selected = current == *state;
            if ui
                .selectable_label(selected, format!("{name} ({key})"))
                .clicked()
            {
                clicked = Some(*state);
            }
        }
    }
    clicked
}
