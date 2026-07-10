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
pub mod inspector;
pub mod joint_inspector;
pub mod reflect_grid;
pub mod settings;
pub mod toolbar;
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
        app.init_resource::<context_menu::ContextMenu>();
        app.init_resource::<console::ScriptConsole>();
        app.add_systems(
            EguiPrimaryContextPass,
            (
                toolbar::toolbar,
                inspector::inspector_window,
                joint_inspector::joint_inspector,
                settings::settings_window,
                console::script_console,
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
