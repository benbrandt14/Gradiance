use bevy::prelude::*;
use gradiance::GamePlugin;

fn main() {
    // TODO: Future Integration:
    // - Initialize Scripting Engine (Rhai/Lua)
    // - Initialize CSG Kernel
    // - Load User Settings
    App::new()
        .add_plugins(GamePlugin)
        .run();
}
