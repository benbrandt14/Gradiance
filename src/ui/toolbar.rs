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

/// Tool-palette entries grouped into sections (Blender-style), in workflow
/// order — pick/move, create a shape, connect with a constraint, then modify.
/// Each entry is `(state, name, key, icon)`, where `icon` is the stem of a PNG
/// under `assets/icons/` (`tool_<icon>.png`). The palette renders the image with
/// the name+key as hover text; the headless test falls back to a text label.
/// Kept as data so `tools_palette_ui` stays a pure projection.
const TOOL_GROUPS: &[(&str, &[(ToolState, &str, &str, &str)])] = &[
    (
        "Select",
        &[
            (ToolState::Select, "Select", "S", "tool_select"),
            (ToolState::Drag, "Drag", "D", "tool_drag"),
        ],
    ),
    (
        "Create",
        &[
            (ToolState::Box, "Box", "B", "tool_box"),
            (ToolState::Circle, "Circle", "C", "tool_circle"),
            (ToolState::Polygon, "Polygon", "P", "tool_polygon"),
        ],
    ),
    (
        "Connect",
        &[
            (ToolState::Hinge, "Hinge", "H", "tool_hinge"),
            (ToolState::Slider, "Prismatic", "R", "tool_prismatic"),
            // The coil icon follows the renamed Spring tool; the rigid-rod
            // Strut borrows the ruler until it gets dedicated art.
            (ToolState::Spring, "Spring", "T", "tool_strut"),
            (ToolState::Strut, "Strut", "L", "measure"),
            (ToolState::Weld, "Weld", "W", "tool_weld"),
            (ToolState::Ground, "Ground", "G", "tool_ground"),
        ],
    ),
    (
        "Modify",
        &[
            (ToolState::Cut, "Cut", "K", "tool_cut"),
            (ToolState::Tracer, "Tracer", "Y", "tool_tracer"),
        ],
    ),
];

/// egui texture ids for each tool's icon, registered at startup from
/// `assets/icons/tool_*.png`. Empty until [`load_tool_icons`] runs (and headless,
/// where there's no render); the palette falls back to text labels then.
#[derive(Resource, Default)]
pub struct ToolIcons {
    map: std::collections::HashMap<ToolState, egui::TextureId>,
}

/// Loads each tool's PNG and registers it with egui, filling [`ToolIcons`]. The
/// `Strong` handle is owned by `EguiUserTextures`, so the image stays loaded.
pub fn load_tool_icons(
    asset_server: Res<AssetServer>,
    mut user_textures: ResMut<bevy_egui::EguiUserTextures>,
    mut icons: ResMut<ToolIcons>,
) {
    for (_group, tools) in TOOL_GROUPS {
        for (state, _name, _key, stem) in *tools {
            let handle: Handle<Image> = asset_server.load(format!("icons/{stem}.png"));
            let id = user_textures.add_image(bevy_egui::EguiTextureHandle::Strong(handle));
            icons.map.insert(*state, id);
        }
    }
}

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
    tool_icons: Res<ToolIcons>,
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

    // A fixed docked strip on the left edge (not a collapsible/movable
    // floating window). Anchored below the transport; hover-reveal + a
    // right-edge placement can follow once the icon set lands.
    egui::Window::new("Tools")
        .title_bar(false)
        .collapsible(false)
        .movable(false)
        .resizable(false)
        .anchor(egui::Align2::LEFT_TOP, [8.0, 120.0])
        .show(ctx, |ui| {
            // Narrow enough that the ~26px icons wrap two per row (a compact
            // Blender-style T-panel strip).
            ui.set_max_width(72.0);
            if let Some(state) = tools_palette_ui(ui, *tool.get(), Some(&tool_icons)) {
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

/// The tool-palette as a docked icon strip, grouped into sections by a
/// separator: highlights `current`, returns a clicked tool. Each tool is an
/// image button (from [`ToolIcons`]) with its name+key on hover; when no icons
/// are registered (headless test) it falls back to a text label. Host-agnostic
/// (pure `Ui` in, choice out) so `tests/it/ui_panels.rs` can exercise it.
pub fn tools_palette_ui(
    ui: &mut egui::Ui,
    current: ToolState,
    icons: Option<&ToolIcons>,
) -> Option<ToolState> {
    let mut clicked = None;
    for (i, (_group, tools)) in TOOL_GROUPS.iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        ui.horizontal_wrapped(|ui| {
            for (state, name, key, _icon) in *tools {
                let selected = current == *state;
                let resp = match icons.and_then(|ic| ic.map.get(state)) {
                    Some(&id) => {
                        let img = egui::Image::new(egui::load::SizedTexture::new(
                            id,
                            egui::vec2(26.0, 26.0),
                        ));
                        ui.add(egui::Button::image(img).selected(selected))
                    }
                    None => ui.selectable_label(selected, format!("{name} ({key})")),
                };
                if resp.on_hover_text(format!("{name} ({key})")).clicked() {
                    clicked = Some(*state);
                }
            }
        });
    }
    clicked
}
