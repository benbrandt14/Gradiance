//! Gradiance binary entry point.

use bevy::prelude::*;
use gradiance::GradiancePlugins;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(GradiancePlugins);
    // Scripting seam (Tier-A authoring path) — off unless built with
    // `--features script`, so the default build never pays steel's weight.
    #[cfg(feature = "script")]
    app.add_plugins(gradiance::script::bridge::ScriptPlugin);
    // Repro workflow: `gradiance <scene.ron>` opens a scene (e.g. a debug
    // snapshot) directly.
    if let Some(path) = std::env::args().nth(1) {
        app.insert_resource(gradiance::persist::StartupScene(path.into()));
    }
    app.run();
}
