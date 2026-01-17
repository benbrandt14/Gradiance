use crate::prelude::*;

pub struct ScriptingPlugin;

impl Plugin for ScriptingPlugin {
    fn build(&self, _app: &mut App) {
        // Spec: Lua integration
        // TODO: Initialize scripting host and register API bindings
    }
}
