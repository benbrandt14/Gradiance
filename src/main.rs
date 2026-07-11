//! Gradiance binary entry point.

use bevy::prelude::*;
use gradiance::GradiancePlugins;
use gradiance::script::bridge::StartupScripts;
use std::path::PathBuf;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(GradiancePlugins);

    // CLI:
    //   `gradiance <scene.ron>`         opens a scene (e.g. a debug snapshot);
    //   `gradiance --script foo.scm …`  runs one or more `.scm` files at
    //                                   startup (scene setup, `register-action`,
    //                                   helpers) — the "author from a file" path.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut scripts = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--script" {
            if let Some(path) = args.get(i + 1) {
                scripts.push(PathBuf::from(path));
                i += 2;
                continue;
            }
        } else {
            app.insert_resource(gradiance::persist::StartupScene(args[i].clone().into()));
        }
        i += 1;
    }
    if !scripts.is_empty() {
        app.insert_resource(StartupScripts(scripts));
    }

    app.run();
}
