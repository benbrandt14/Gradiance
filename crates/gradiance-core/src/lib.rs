//! Cross-cutting foundations: constants, identity, states, and shared units.
//!
//! Everything in `core` is dependency-light and importable from any layer.

pub mod constants;
pub mod ids;
pub mod messages;
pub mod script_tools;
pub mod states;
pub mod units;

use bevy::prelude::*;

/// Registers the app-wide states, the [`ids::IdIndex`], and the
/// [`script_tools`] table and message (shared by the script and tool layers).
#[derive(Default)]
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<states::GameState>();
        app.init_state::<states::ToolState>();
        app.init_resource::<ids::IdIndex>();
        app.init_resource::<script_tools::ScriptTools>();
        app.add_message::<script_tools::ScriptToolFired>();
    }
}
