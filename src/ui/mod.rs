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
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass};

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
        app.init_resource::<probe::ProbePanel>();
        app.init_resource::<signals::SignalsPanel>();
        app.init_resource::<node_graph::NodeGraph>();
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
) -> Result {
    let ctx = contexts.ctx_mut()?;
    over_ui.0 = ctx.is_pointer_over_egui();
    keyboard.0 = ctx.egui_wants_keyboard_input();
    Ok(())
}
