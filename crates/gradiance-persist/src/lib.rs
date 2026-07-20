//! Persistence: scene files on disk, save/load requests, and debugging
//! snapshots.
//!
//! Division of labor: the records and the RON format (versioning,
//! migrations, encode/decode) live in [`gradiance_scene`]; `SceneRecord::apply`
//! (the actual world mutation) runs inside the command layer — **loading is
//! an undoable command**. This module only touches disk, shows dialogs, and
//! turns requests into `LoadSceneIntent`s.
//!
//! Debug/repro aids:
//! - `SnapshotRequest` (F12) dumps the live scene to
//!   `snapshots/gradiance-<timestamp>.ron` — attach it to a bug report.
//! - `gradiance <scene.ron>` loads a scene at startup, so a snapshot
//!   reproduces a session in one command.

#[cfg(feature = "dev")]
pub mod flight;

use bevy::prelude::*;
use gradiance_command::intent::LoadSceneIntent;
use gradiance_core::messages::drain;
use gradiance_scene::{PersistError, SceneRecord, from_ron, to_ron};
use std::path::{Path, PathBuf};

/// Request to save the scene (`path: None` → remembered path, then dialog).
#[derive(Message, Debug, Clone)]
pub struct SaveSceneRequest {
    /// Target file; `None` uses `LastScenePath` or asks.
    pub path: Option<PathBuf>,
}

/// Request to load a scene (`path: None` → dialog).
#[derive(Message, Debug, Clone)]
pub struct LoadSceneRequest {
    /// Source file; `None` asks.
    pub path: Option<PathBuf>,
}

/// Request to dump a timestamped debugging snapshot.
#[derive(Message, Debug, Clone, Default)]
pub struct SnapshotRequest {
    /// Directory override (defaults to `./snapshots`).
    pub dir: Option<PathBuf>,
}

/// The last save/load path (Ctrl+S saves here without asking).
#[derive(Resource, Default, Debug)]
pub struct LastScenePath(pub Option<PathBuf>);

/// Scene file to load on startup (set from the CLI for reproductions).
#[derive(Resource, Debug, Clone)]
pub struct StartupScene(pub PathBuf);

/// Default session-autosave file, written on exit and reopened by
/// `gradiance --resume` (gitignored; crash-free sessions always leave one).
pub const AUTOSAVE_FILE: &str = ".gradiance-session.ron";

/// Where the exit autosave is written (tests point this at a temp dir).
#[derive(Resource, Debug, Clone)]
pub struct AutosavePath(pub PathBuf);

impl Default for AutosavePath {
    fn default() -> Self {
        Self(PathBuf::from(AUTOSAVE_FILE))
    }
}

/// Installs persistence handling.
#[derive(Default)]
pub struct PersistPlugin;

impl Plugin for PersistPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SaveSceneRequest>();
        app.add_message::<LoadSceneRequest>();
        app.add_message::<SnapshotRequest>();
        app.init_resource::<LastScenePath>();
        app.init_resource::<AutosavePath>();
        app.add_systems(Startup, queue_startup_scene);
        app.add_systems(
            Update,
            handle_persist_requests.before(gradiance_command::CommandDispatchSet),
        );
        app.add_systems(Last, autosave_on_exit);
        // Dev-only: F9 dumps the flight recorder ring buffer to RON.
        #[cfg(feature = "dev")]
        app.add_systems(Update, flight::dump_flight_recorder);
    }
}

fn queue_startup_scene(
    startup: Option<Res<StartupScene>>,
    mut load: MessageWriter<LoadSceneRequest>,
) {
    if let Some(startup) = startup {
        info!(path = %startup.0.display(), "loading startup scene");
        load.write(LoadSceneRequest {
            path: Some(startup.0.clone()),
        });
    }
}

/// Serializes `scene` and writes it to `path` — the one save flow shared by
/// explicit saves, debug snapshots, and the exit autosave.
fn write_scene_file(scene: &SceneRecord, path: &Path) -> Result<(), PersistError> {
    Ok(std::fs::write(path, to_ron(scene)?)?)
}

/// Drains persistence requests: saves capture-and-write, loads parse into
/// [`LoadSceneIntent`] (which dispatch turns into the undoable command).
pub fn handle_persist_requests(world: &mut World) {
    let saves: Vec<SaveSceneRequest> = drain(world);
    let loads: Vec<LoadSceneRequest> = drain(world);
    let snapshots: Vec<SnapshotRequest> = drain(world);

    for request in saves {
        let path = request
            .path
            .or_else(|| world.resource::<LastScenePath>().0.clone())
            .or_else(ask_save_path);
        let Some(path) = path else { continue };
        let scene = SceneRecord::capture(world);
        match write_scene_file(&scene, &path) {
            Ok(()) => {
                info!(path = %path.display(), "scene saved");
                world.resource_mut::<LastScenePath>().0 = Some(path);
            }
            Err(e) => warn!(path = %path.display(), error = %e, "scene save failed"),
        }
    }

    for request in loads {
        let path = request.path.or_else(ask_load_path);
        let Some(path) = path else { continue };
        match std::fs::read_to_string(&path)
            .map_err(PersistError::from)
            .and_then(|t| from_ron(&t))
        {
            Ok(scene) => {
                world.resource_mut::<LastScenePath>().0 = Some(path);
                world.write_message(LoadSceneIntent { scene });
            }
            Err(e) => warn!(path = %path.display(), error = %e, "scene load failed"),
        }
    }

    for request in snapshots {
        let dir = request.dir.unwrap_or_else(|| PathBuf::from("snapshots"));
        let scene = SceneRecord::capture(world);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let path = dir.join(format!("gradiance-{stamp}.ron"));
        let result = std::fs::create_dir_all(&dir)
            .map_err(PersistError::from)
            .and_then(|()| write_scene_file(&scene, &path));
        match result {
            Ok(()) => info!(path = %path.display(), "debug snapshot written"),
            Err(e) => warn!(error = %e, "snapshot failed"),
        }
    }
}

/// Writes the session autosave when the app is exiting (Tier-2 dev loop:
/// `--resume` reopens it, so a rebuild lands back in the same scene).
///
/// An empty scene is not written — closing immediately after launch must not
/// clobber the previous session.
pub fn autosave_on_exit(world: &mut World) {
    let exiting = world
        .get_resource::<Messages<AppExit>>()
        .is_some_and(|m| !m.is_empty());
    if !exiting {
        return;
    }
    let scene = SceneRecord::capture(world);
    if scene.bodies.is_empty() && scene.joints.is_empty() {
        return;
    }
    let path = world.resource::<AutosavePath>().0.clone();
    match write_scene_file(&scene, &path) {
        Ok(()) => info!(path = %path.display(), "session autosaved (reopen with --resume)"),
        Err(e) => warn!(path = %path.display(), error = %e, "session autosave failed"),
    }
}

fn ask_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Gradiance scene", &["ron"])
        .set_file_name("scene.ron")
        .save_file()
}

fn ask_load_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Gradiance scene", &["ron"])
        .pick_file()
}
