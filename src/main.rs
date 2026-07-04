//! Gradiance binary entry point.

use bevy::prelude::*;
use gradiance::GradiancePlugins;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GradiancePlugins)
        .run();
}
