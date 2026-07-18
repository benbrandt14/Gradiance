//! The egui UI layer — the **only** module allowed to import `egui`.
//!
//! Testability strategy: this layer contains no decisions worth testing.
//! Every mutation of authored content is a typed intent (headless-tested
//! at the command layer); every widget is a thin projection of component
//! copies. The one sanctioned direct mutation is *editor settings
//! resources* (`GridSettings`, `SnapConfig`, `SimSettings`) — non-authored,
//! non-undoable configuration that downstream seams consume via
//! `Changed<>`/`resource_changed` (physics applies `SimSettings`; UI never
//! touches avian).

pub mod console;
pub mod context_menu;
pub mod depth_panel;
pub mod dock;
pub mod inspector;
pub mod joint_inspector;
pub mod labels;
pub mod node_graph;
pub mod plot;
pub mod ports;
pub mod probe;
pub mod reflect_grid;
pub mod settings;
pub mod signals;
pub mod toolbar;
pub mod view_cube;
pub mod widgets;

use crate::interaction::PointerOverUi;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

/// Screen rects claimed this frame by the **background-layer** docked panels
/// (the right dock, the node-graph dock). egui draws these into a root `Ui` on
/// [`egui::LayerId::background()`], which `is_pointer_over_egui` deliberately
/// ignores — so without this the scene tools/camera would react to input over
/// them (the "click-through"). Each panel pushes its rect while open;
/// `capture_pointer_over_ui` folds them into [`PointerOverUi`] and clears the
/// list for the next frame.
#[derive(Resource, Default)]
pub struct PanelRects(pub Vec<egui::Rect>);

impl PanelRects {
    /// Records a docked panel's occupied screen rect for this frame.
    pub fn push(&mut self, rect: egui::Rect) {
        self.0.push(rect);
    }
}

/// Whether `pos` falls inside any recorded background-layer panel rect — pure,
/// so the click-through gate is unit-testable without a window.
fn pointer_over_panels(rects: &[egui::Rect], pos: Option<egui::Pos2>) -> bool {
    pos.is_some_and(|p| rects.iter().any(|r| r.contains(p)))
}

/// Installs egui and the editor panels (no-op headless).
#[derive(Default)]
pub struct GradianceUiPlugin;

impl Plugin for GradianceUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::render::RenderPlugin>() {
            return;
        }
        app.add_plugins(EguiPlugin::default());
        app.init_resource::<settings::SettingsWindow>();
        app.init_resource::<inspector::InspectorPanel>();
        app.init_resource::<context_menu::ContextMenu>();
        app.init_resource::<depth_panel::DepthPanel>();
        app.init_resource::<console::ScriptConsole>();
        app.init_resource::<plot::PlotPanel>();
        app.init_resource::<plot::PlotConfig>();
        app.init_resource::<probe::ProbePanel>();
        app.init_resource::<signals::SignalsPanel>();
        app.init_resource::<node_graph::NodeGraph>();
        app.init_resource::<PanelRects>();
        app.add_systems(
            EguiPrimaryContextPass,
            (
                toolbar::toolbar,
                view_cube::view_cube,
                dock::right_dock,
                labels::draw_workspace_labels,
                inspector::inspector_window,
                joint_inspector::joint_inspector,
                settings::settings_window,
                plot::plot_panel,
                node_graph::node_graph_panel,
                probe::probe_panel,
                context_menu::context_menu,
                capture_pointer_over_ui,
            )
                .chain(),
        );
        app.add_systems(Update, context_menu::open_context_menu);
    }
}

/// Publishes whether egui wants the pointer/keyboard, for shortcut and
/// tool/camera gating.
fn capture_pointer_over_ui(
    mut contexts: EguiContexts,
    mut over_ui: ResMut<PointerOverUi>,
    mut keyboard: ResMut<crate::interaction::KeyboardCaptured>,
    mut panels: ResMut<PanelRects>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // `is_pointer_over_egui` covers normal areas/windows but not the
    // background-layer docked panels — fold their rects in so input over them
    // doesn't leak to the scene. Runs last in the pass; clear for next frame.
    let pointer = ctx.pointer_latest_pos();
    over_ui.0 = ctx.is_pointer_over_egui() || pointer_over_panels(&panels.0, pointer);
    keyboard.0 = ctx.egui_wants_keyboard_input();
    panels.0.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::pointer_over_panels;
    use bevy_egui::egui::{Pos2, Rect};

    #[test]
    fn a_pointer_inside_a_panel_rect_counts_as_over_ui() {
        let dock = Rect::from_min_max(Pos2::new(100.0, 0.0), Pos2::new(200.0, 300.0));
        let rects = [dock];
        assert!(pointer_over_panels(&rects, Some(Pos2::new(150.0, 100.0))));
        // Over the free scene area, not any panel.
        assert!(!pointer_over_panels(&rects, Some(Pos2::new(50.0, 100.0))));
        // No pointer (cursor off-window) is never "over UI".
        assert!(!pointer_over_panels(&rects, None));
        // No panels drawn this frame.
        assert!(!pointer_over_panels(&[], Some(Pos2::new(150.0, 100.0))));
    }
}
