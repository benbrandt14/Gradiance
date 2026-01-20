//! User Interface (UI) systems.
//!
//! This module implements the editor interface using `bevy_egui`.
//! It includes the sidebar for tools, the inspector for properties,
//! the context menu, and the grid system.

use crate::prelude::*;
use bevy_egui::EguiPlugin;

pub mod context_menu;
pub mod diagnostics;
pub mod grid;
pub mod icons;
pub mod inspector;
pub mod panels;

/// Plugin for the Editor User Interface.
///
/// Initializes `bevy_egui` and registers sub-plugins for panels, inspector, context menu, and grid.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Egui setup
        app.add_plugins(EguiPlugin);

        app.add_plugins((
            icons::IconsPlugin,
            panels::PanelsPlugin,
            inspector::InspectorPlugin,
            context_menu::ContextMenuPlugin,
            grid::GridPlugin,
            diagnostics::DiagnosticsPlugin,
        ));
    }
}
