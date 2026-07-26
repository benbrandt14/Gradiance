//! Cross-cutting foundations: constants, identity, states, and shared units.
//!
//! Everything in `core` is dependency-light and importable from any layer.

pub mod constants;
pub mod ids;
pub mod messages;
pub mod states;
pub mod units;

use bevy::prelude::*;

/// Registers the app-wide states and the [`ids::IdIndex`].
#[derive(Default)]
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<states::GameState>();
        app.init_state::<states::ToolState>();
        app.init_state::<states::EditorMode>();
        app.init_state::<states::SketchTool>();
        app.init_resource::<ids::IdIndex>();
    }
}
