//! User Interface (UI) systems.
//!
//! This module implements the editor interface using `bevy_egui`.
//! It includes the sidebar for tools, the inspector for properties,
//! the context menu, and the grid system.

use crate::prelude::*;
use crate::input::PointerOverUi;
use bevy_egui::{EguiContexts, EguiPlugin};

pub mod context_menu;
pub mod diagnostics;
pub mod grid;
pub mod icons;
pub mod inspector;
pub mod inspector_handlers;
pub mod menu;
pub mod panels;

/// Plugin for the Editor User Interface.
///
/// Initializes `bevy_egui` and registers sub-plugins for panels, inspector, context menu, and grid.
pub struct GradianceUiPlugin;

impl Plugin for GradianceUiPlugin {
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
            menu::MenuPlugin,
        ))
        .add_systems(PreUpdate, update_pointer_over_ui)
        .add_systems(Update, inspector_handlers::handle_property_change_event);
    }
}

fn update_pointer_over_ui(mut contexts: EguiContexts, mut pointer_over_ui: ResMut<PointerOverUi>) {
    if let Some(ctx) = contexts.try_ctx_mut() {
        pointer_over_ui.0 = ctx.is_pointer_over_area();
    } else {
        pointer_over_ui.0 = false;
    }
}
