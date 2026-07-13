//! Persistence: RON scene files, save/load requests, and debugging
//! snapshots.
//!
//! Division of labor: `SceneRecord` (capture/apply, the actual world
//! mutation) lives in the command layer — **loading is an undoable
//! command**. This module only serializes, touches disk, shows dialogs,
//! and turns requests into `LoadSceneIntent`s.
//!
//! Debug/repro aids:
//! - `SnapshotRequest` (F12) dumps the live scene to
//!   `snapshots/gradiance-<timestamp>.ron` — attach it to a bug report.
//! - `gradiance <scene.ron>` loads a scene at startup, so a snapshot
//!   reproduces a session in one command.
//! - Files carry `version` (format migration gate) and `app_version`
//!   (which build wrote it).

#[cfg(feature = "dev")]
pub mod flight;

use crate::command::intent::LoadSceneIntent;
use crate::command::snapshot::SceneRecord;
use bevy::prelude::*;
use std::path::PathBuf;

/// Scene format version accepted by this build.
///
/// v2: the de-adapter collapse — authored physics is now avian components
/// serialized directly (`docs/physics-deadapter-decision.md`). v1 files do
/// not load (save-format stability across the collapse is not a goal).
///
/// v3: the weld rework (M20) — `JointKind::Weld` no longer exists (the weld
/// tool merges bodies or makes them static), so v2 files carrying weld
/// joints do not load.
///
/// v4: the strut rework — `JointKind::Spring` is authored as `rest_length`
/// with an optional `range` clamp (was `bounds`), so v3 files carrying
/// struts do not load.
///
/// v5: continuous depth (V3) — bodies author a `DepthBand` instead of a
/// `LayerMask32`. v4 files migrate on load: each mask's occupied bit range
/// maps to the equivalent band; non-default *filters* are dropped with a
/// warning (checkbox filter art is unrepresentable by design — collision
/// is depth overlap).
pub const FORMAT_VERSION: u32 = 5;

/// What went wrong while persisting.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// Filesystem failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// RON serialization failure.
    #[error("serialize: {0}")]
    Serialize(#[from] ron::Error),
    /// RON parse failure.
    #[error("parse: {0}")]
    Parse(#[from] ron::de::SpannedError),
    /// The file's format version is not supported.
    #[error("unsupported scene version {0} (this build reads {FORMAT_VERSION})")]
    Version(u32),
}

/// Serializes a scene to pretty RON (deterministic for identical scenes).
pub fn to_ron(scene: &SceneRecord) -> Result<String, PersistError> {
    Ok(ron::ser::to_string_pretty(
        scene,
        ron::ser::PrettyConfig::new(),
    )?)
}

/// Parses and version-checks a scene, migrating supported old versions.
pub fn from_ron(text: &str) -> Result<SceneRecord, PersistError> {
    let mut scene: SceneRecord = ron::from_str(text)?;
    match scene.version {
        FORMAT_VERSION => Ok(scene),
        4 => {
            migrate_v4_layers(&mut scene);
            Ok(scene)
        }
        v => Err(PersistError::Version(v)),
    }
}

/// v4 → v5: each body's legacy layer mask becomes the equivalent depth
/// band. Custom filter masks cannot be represented (collision is depth
/// overlap now) and are dropped with a warning.
fn migrate_v4_layers(scene: &mut SceneRecord) {
    use crate::domain::depth::DepthBand;
    for body in &mut scene.bodies {
        if let Some(mask) = body.layers.take() {
            body.depth = mask
                .occupied_range()
                .map_or_else(DepthBand::default, |(min, max)| {
                    DepthBand::from_bit_range(min, max)
                });
            if mask.filters != u32::MAX {
                warn!(
                    id = %body.id.0,
                    "v4 custom collision filters dropped on migration \
                     (collision is depth overlap in v5)"
                );
            }
        }
    }
    scene.version = FORMAT_VERSION;
}

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
            handle_persist_requests.before(crate::command::CommandDispatchSet),
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
        match to_ron(&scene).map(|text| std::fs::write(&path, text)) {
            Ok(Ok(())) => {
                info!(path = %path.display(), "scene saved");
                world.resource_mut::<LastScenePath>().0 = Some(path);
            }
            Ok(Err(e)) => warn!(path = %path.display(), error = %e, "scene save failed"),
            Err(e) => warn!(error = %e, "scene serialize failed"),
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
            .and_then(|()| Ok(std::fs::write(&path, to_ron(&scene)?)?));
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
    match to_ron(&scene).map(|text| std::fs::write(&path, text)) {
        Ok(Ok(())) => info!(path = %path.display(), "session autosaved (reopen with --resume)"),
        Ok(Err(e)) => warn!(path = %path.display(), error = %e, "session autosave failed"),
        Err(e) => warn!(error = %e, "session autosave serialize failed"),
    }
}

fn drain<M: Message>(world: &mut World) -> Vec<M> {
    world
        .get_resource_mut::<Messages<M>>()
        .map(|mut m| m.drain().collect())
        .unwrap_or_default()
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
