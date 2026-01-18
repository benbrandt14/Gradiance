//! UI Plugin

use crate::prelude::*;
use bevy_egui::EguiPlugin;

pub mod context_menu;
pub mod grid;
pub mod inspector;
pub mod panels;

/// Plugin that initializes all UI components.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin); // .default() removed for Bevy 0.14? Or EguiPlugin struct change?
        // Checking EguiPlugin for Bevy 0.14/Egui 0.28.
        // It's likely just `app.add_plugins(EguiPlugin);` if it's a struct unit or has default.
        // If it fails, I'll check docs.

        app.add_plugins((
            panels::PanelsPlugin,
            inspector::InspectorPlugin,
            context_menu::ContextMenuPlugin,
            grid::GridPlugin,
        ));
    }
}
