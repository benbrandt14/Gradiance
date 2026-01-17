use crate::prelude::*;

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, _app: &mut App) {
        // Register individual tool systems here
    }
}

// Trait for tools could go here, or just individual systems handling ToolState
pub trait Tool {
    fn name(&self) -> &str;
    // ...
}
