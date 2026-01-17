use crate::prelude::*;
use bevy_egui::{EguiPlugin};

pub mod context_menu;
pub mod inspector;
pub mod panels;
pub mod grid;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // Egui setup
        app.add_plugins(EguiPlugin::default());

        app.add_plugins((
            panels::PanelsPlugin,
            inspector::InspectorPlugin,
            context_menu::ContextMenuPlugin,
            grid::GridPlugin,
        ));
    }
}
