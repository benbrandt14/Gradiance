//! Tool palette (left T-panel of icons) + transport strip (play/pause, scale
//! frame, 2D-view home). Undo/redo and panel toggles live in the menu bar.

use crate::fonts::glyph;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::states::{GameState, ToolState};
use gradiance_interaction::tools::handles::ScaleFrame;

/// The editor panels the transport strip toggles, bundled so the toolbar
/// system stays under Bevy's system-parameter limit.
#[derive(SystemParam)]
pub struct Panels<'w> {
    /// Settings window.
    pub settings: ResMut<'w, crate::settings::SettingsWindow>,
    /// Properties inspector dock pane (toggle).
    pub inspector: ResMut<'w, crate::inspector::InspectorPanel>,
    /// Live plot panel.
    pub plot: ResMut<'w, crate::plot::PlotPanel>,
    /// Physics probe panel.
    pub probe: ResMut<'w, crate::probe::ProbePanel>,
    /// Signals dock section.
    pub signals: ResMut<'w, crate::signals::SignalsPanel>,
    /// Node-graph canvas.
    pub node_graph: ResMut<'w, crate::node_graph::NodeGraph>,
    /// Object tree (outliner).
    pub outliner: ResMut<'w, crate::outliner::ObjectTreePanel>,
    /// Scripting console.
    pub console: ResMut<'w, crate::console::ScriptConsole>,
    /// Debug overlays (field vectors).
    pub debug: ResMut<'w, gradiance_domain::settings::DebugSettings>,
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
            (ToolState::Strut, "Strut", "T", "tool_strut"),
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
    mut rig: ResMut<gradiance_interaction::camera::CameraRig>,
    panel_rects: Res<crate::PanelRects>,
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
            // Only *simulation/view* controls that aren't in the menus. Undo/
            // Redo (Edit menu), panel toggles + Fields + Settings (View menu)
            // used to live here too — that duplication is removed.
            ui.horizontal(|ui| {
                let playing = *game.get() == GameState::Playing;
                if ui
                    .button(if playing {
                        format!("{} Pause", glyph::PAUSE)
                    } else {
                        format!("{} Play", glyph::PLAY)
                    })
                    .clicked()
                {
                    next_game.set(if playing {
                        GameState::Paused
                    } else {
                        GameState::Playing
                    });
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
                    .add_enabled(!rig.is_flat(), egui::Button::new("2D view"))
                    .on_hover_text("return the camera to the flat 2D view (Home)")
                    .clicked()
                {
                    rig.glide_home();
                }
            });
        });

    // A fixed docked strip on the left edge (not a collapsible/movable floating
    // window), slightly translucent so the scene reads behind it.
    let tools_frame = egui::Frame::window(&ctx.global_style()).multiply_with_opacity(0.82);
    egui::Window::new("Tools")
        .title_bar(false)
        .collapsible(false)
        .movable(false)
        .resizable(false)
        .frame(tools_frame)
        .anchor(egui::Align2::LEFT_TOP, [8.0, 120.0])
        .show(ctx, |ui| {
            // A single column about twice the icon width (icons ~26px), so each
            // sits in a roomy cell — a compact Blender-style T-panel strip.
            ui.set_max_width(52.0);
            if let Some(state) = tools_palette_ui(ui, *tool.get(), Some(&tool_icons)) {
                next_tool.set(state);
            }
        });
    Ok(())
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
