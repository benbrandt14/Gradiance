//! Scripting support (Planned).
//!
//! This module will house the Lua integration using `bevy_mod_scripting`,
//! allowing users to write scripts to control entities and game logic.

use crate::prelude::*;

/// Plugin for scripting integration.
///
/// **Note:** This is currently a placeholder and does not yet register any scripting systems.
pub struct ScriptingPlugin;

impl Plugin for ScriptingPlugin {
    fn build(&self, _app: &mut App) {
        // Spec: Lua integration
        // TODO: Initialize scripting host and register API bindings
    }
}
