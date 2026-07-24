//! Gradiance binary entry point.

use bevy::prelude::*;
use gradiance::GradiancePlugins;
use gradiance::script::bridge::StartupScripts;
use std::path::PathBuf;

// Under the `tracy` feature, `bevy_log`'s `trace_tracy_memory` installs a
// Tracy-tracking global allocator for live allocation profiling, so we yield
// the global allocator to it; every other build uses mimalloc.
#[cfg(not(feature = "tracy"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(GradiancePlugins);

    // Opt-in developer diagnostics overlay (`cargo run --features diagnostics`).
    #[cfg(feature = "diagnostics")]
    app.add_plugins(gradiance::diagnostics::DiagnosticsPlugin);

    // CLI:
    //   `gradiance <scene.ron>`         opens a scene (e.g. a debug snapshot);
    //   `gradiance --script foo.scm …`  runs one or more `.scm` files at
    //                                   startup (scene setup, `register-action`,
    //                                   helpers) and hot-reloads them on change;
    //   `gradiance --resume`            reopens the exit autosave
    //                                   (`.gradiance-session.ron`).
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
        } else if args[i] == "--resume" {
            let autosave = PathBuf::from(gradiance::persist::AUTOSAVE_FILE);
            if autosave.exists() {
                app.insert_resource(gradiance::persist::StartupScene(autosave));
            } else {
                eprintln!("--resume: no {} to reopen", autosave.display());
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
