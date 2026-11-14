//! Tool palette (left T-panel of icons) + transport strip (play/pause, scale
//! frame, 2D-view home). Undo/redo and panel toggles live in the menu bar.

use crate::fonts::glyph;
use crate::icons::{self, Icon};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::script_tools::{ScriptToolSpec, ScriptTools};
use gradiance_core::states::{GameState, ToolState};
use gradiance_interaction::tools::handles::ScaleFrame;

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
            (ToolState::Spring, "Spring", "T", "tool_strut"),
            (ToolState::Rod, "Rod", "L", "wire"),
            (ToolState::Weld, "Weld", "W", "tool_weld"),
            (ToolState::Ground, "Ground", "G", "tool_ground"),
        ],
    ),
    (
        "Modify",
        &[
            (ToolState::Cut, "Cut", "K", "tool_cut"),
            (ToolState::Tracer, "Tracer", "N", "tool_tracer"),
        ],
    ),
];

/// The icon every script-registered tool shows (an existing one: custom
/// icons for script tools are out of scope). Its name is the hover text and
/// the caption under the icon.
const SCRIPT_TOOL_ICON: &str = "sketch";

/// egui texture ids for each tool's icon, registered at startup from
/// `assets/icons/tool_*.png`. Empty until [`load_tool_icons`] runs (and headless,
/// where there's no render); the palette falls back to text labels then.
#[derive(Resource, Default)]
pub struct ToolIcons {
    map: std::collections::HashMap<ToolState, egui::TextureId>,
    /// The shared icon of every script tool.
    script: Option<egui::TextureId>,
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
    let handle: Handle<Image> = asset_server.load(format!("icons/{SCRIPT_TOOL_ICON}.png"));
    icons.script = Some(user_textures.add_image(bevy_egui::EguiTextureHandle::Strong(handle)));
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
    icons: Res<crate::icons::Icons>,
    script_tools: Res<ScriptTools>,
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
                // Image first, glyph as the fallback label: the transport is
                // always on screen, so it is the one control most worth a real
                // picture — and `▶` is not in any bundled font, so the glyph
                // path alone was a tofu box here for the editor's whole life.
                let (icon, label) = if playing {
                    (Icon::Pause, format!("{} Pause", glyph::PAUSE))
                } else {
                    (Icon::Play, format!("{} Play", glyph::PLAY))
                };
                if icons::icon_text_button(ui, &icons, icon, &label).clicked() {
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
                    .add_enabled(
                        !rig.is_flat(),
                        match icons.get(Icon::HomeView) {
                            Some(id) => egui::Button::image_and_text(
                                egui::load::SizedTexture::new(
                                    id,
                                    egui::vec2(icons::ICON_SIZE, icons::ICON_SIZE),
                                ),
                                "2D view",
                            ),
                            None => egui::Button::new("2D view"),
                        },
                    )
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
            if let Some(state) =
                tools_palette_ui(ui, *tool.get(), Some(&tool_icons), &script_tools.0)
            {
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
///
/// Tools a script registered (`register-tool`) follow the built-ins in a
/// "Scripts" section: one shared icon each, captioned with the tool's name,
/// selecting `ToolState::Script(i)`.
pub fn tools_palette_ui(
    ui: &mut egui::Ui,
    current: ToolState,
    icons: Option<&ToolIcons>,
    script_tools: &[ScriptToolSpec],
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
    if let Some(state) = script_tools_ui(ui, current, icons.and_then(|ic| ic.script), script_tools)
    {
        clicked = Some(state);
    }
    clicked
}

/// The palette's "Scripts" section (nothing when no tool is registered).
fn script_tools_ui(
    ui: &mut egui::Ui,
    current: ToolState,
    icon: Option<egui::TextureId>,
    script_tools: &[ScriptToolSpec],
) -> Option<ToolState> {
    let mut clicked = None;
    for (i, tool) in script_tools.iter().enumerate() {
        // The state carries a u16 index; the script layer never registers
        // past it, but a palette must not wrap around to another tool.
        let Ok(index) = u16::try_from(i) else { break };
        if i == 0 {
            ui.separator();
        }
        let state = ToolState::Script(index);
        let selected = current == state;
        let hover = format!("{} (script tool, {} clicks)", tool.name, tool.points);
        ui.vertical_centered(|ui| {
            let resp = match icon {
                Some(id) => {
                    let img =
                        egui::Image::new(egui::load::SizedTexture::new(id, egui::vec2(26.0, 26.0)));
                    let resp = ui.add(egui::Button::image(img).selected(selected));
                    ui.small(&tool.name);
                    resp
                }
                None => ui.selectable_label(selected, &tool.name),
            };
            if resp.on_hover_text(hover).clicked() {
                clicked = Some(state);
            }
        });
    }
    clicked
}
