//! The scene browser's disk side (brief S-17): the folder the Scenes panel
//! shows, its listing, the thumbnail sidecar a save writes, and the
//! browse, rename and trash operations.
//!
//! A thumbnail is a sidecar PNG next to its scene (`a.ron` → `a.png`), never
//! inside the RON (single-format rule). `command::thumbnail` draws it from
//! the records. The folder is workstation config: a resource the person
//! sets, never saved in a scene and never undone (decision
//! `scene-browser-folder-not-saved`). Until it is set, the panel shows the
//! folder of the open scene, else the working directory.
//!
//! The operations only queue a [`SceneFileRequest`]; the persist system
//! does the disk work, so an operation's lowering stays pure. Trash moves
//! a scene and its sidecar into a `.trash` folder beside them (decision
//! `scene-trash-folder`): no platform trash API, and a mistake is one file
//! move away from undone.

use bevy::prelude::*;
use gradiance_command::thumbnail::{THUMB_HEIGHT, THUMB_WIDTH, draw};
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam};
use gradiance_scene::SceneRecord;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The folder a trashed scene moves into, beside the scene.
pub const TRASH_DIR: &str = ".trash";

/// The folder the Scenes panel shows (`None`: the open scene's folder,
/// else the working directory). Workstation config: never saved, never
/// undone.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ScenesFolder(pub Option<PathBuf>);

impl ScenesFolder {
    /// The folder to list: this one, else `scene_folder`, else `.`.
    #[must_use]
    pub fn resolve(&self, scene_folder: Option<&Path>) -> PathBuf {
        self.0
            .clone()
            .or_else(|| scene_folder.map(Path::to_path_buf))
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// One scene file in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneEntry {
    /// The file name without `.ron`.
    pub name: String,
    /// The scene file.
    pub path: PathBuf,
    /// Its thumbnail sidecar, when there is one.
    pub thumbnail: Option<PathBuf>,
}

/// The sidecar thumbnail path of the scene at `path` (`a.ron` → `a.png`).
#[must_use]
pub fn thumbnail_path(path: &Path) -> PathBuf {
    path.with_extension("png")
}

/// The `.ron` files in `folder`, by name, each with its sidecar if any.
/// An unreadable folder lists nothing.
#[must_use]
pub fn list_scenes(folder: &Path) -> Vec<SceneEntry> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut entries: Vec<SceneEntry> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && crate::ops::has_extension(p, "ron"))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            let sidecar = thumbnail_path(&path);
            Some(SceneEntry {
                name,
                thumbnail: sidecar.is_file().then_some(sidecar),
                path,
            })
        })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

/// Writes the thumbnail of `scene` beside the scene file at `path`.
///
/// # Errors
///
/// The encoder's or the disk's error, as text.
pub fn write_thumbnail(scene: &SceneRecord, path: &Path) -> Result<(), String> {
    let png = thumbnail_png(scene)?;
    std::fs::write(thumbnail_path(path), png).map_err(|e| e.to_string())
}

/// The thumbnail of `scene`, encoded as PNG bytes.
///
/// # Errors
///
/// The encoder's error, as text.
pub fn thumbnail_png(scene: &SceneRecord) -> Result<Vec<u8>, String> {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let image = Image::new(
        Extent3d {
            width: THUMB_WIDTH,
            height: THUMB_HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        draw(scene),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let format = bevy::image::ImageFormat::Png
        .as_image_crate_format()
        .ok_or_else(|| "this build has no PNG encoder".to_owned())?;
    let mut png = Vec::new();
    image
        .try_into_dynamic()
        .map_err(|e| e.to_string())?
        .write_to(&mut std::io::Cursor::new(&mut png), format)
        .map_err(|e| e.to_string())?;
    Ok(png)
}

/// A disk request from the scene-browser operations.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum SceneFileRequest {
    /// Show this folder (`None`: ask with a folder dialog).
    Browse(Option<PathBuf>),
    /// Rename a scene and its sidecar.
    Rename {
        /// The scene file now.
        from: PathBuf,
        /// Its new path.
        to: PathBuf,
    },
    /// Move a scene and its sidecar into the trash folder beside it.
    Trash(PathBuf),
}

/// Registers the browser's resource, message and operations.
pub(crate) fn install(app: &mut App) {
    app.init_resource::<ScenesFolder>();
    app.add_message::<SceneFileRequest>();
    let mut registry = app.world_mut().resource_mut::<OpRegistry>();
    registry.register::<BrowseDir>();
    registry.register::<RenameScene>();
    registry.register::<TrashScene>();
}

/// Does each queued [`SceneFileRequest`]; a failure is logged, not raised
/// (the panel lists what is on disk either way).
pub(crate) fn handle_requests(world: &mut World, requests: Vec<SceneFileRequest>) {
    for request in requests {
        match request {
            SceneFileRequest::Browse(folder) => {
                let folder = folder.or_else(|| rfd::FileDialog::new().pick_folder());
                if let Some(folder) = folder {
                    world.insert_resource(ScenesFolder(Some(folder)));
                }
            }
            SceneFileRequest::Rename { from, to } => match rename(&from, &to) {
                Ok(()) => {
                    info!(from = %from.display(), to = %to.display(), "scene renamed");
                    let mut last = world.resource_mut::<crate::LastScenePath>();
                    if last.0.as_deref() == Some(from.as_path()) {
                        last.0 = Some(to);
                    }
                }
                Err(e) => warn!(path = %from.display(), error = %e, "scene rename failed"),
            },
            SceneFileRequest::Trash(path) => match trash(&path) {
                Ok(moved) => {
                    info!(path = %path.display(), to = %moved.display(), "scene moved to trash");
                    let mut last = world.resource_mut::<crate::LastScenePath>();
                    if last.0.as_deref() == Some(path.as_path()) {
                        last.0 = None;
                    }
                }
                Err(e) => warn!(path = %path.display(), error = %e, "scene trash failed"),
            },
        }
    }
}

/// Renames the scene `from` to `to`, and its sidecar with it. Refuses to
/// replace an existing file.
///
/// # Errors
///
/// `to` exists, or the disk refused.
pub fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} exists", to.display()),
        ));
    }
    std::fs::rename(from, to)?;
    let (old, new) = (thumbnail_path(from), thumbnail_path(to));
    if old.is_file() && !new.exists() {
        std::fs::rename(old, new)?;
    }
    Ok(())
}

/// Moves the scene at `path` and its sidecar into [`TRASH_DIR`] beside it;
/// a name already in the trash gets a numbered suffix. Returns where the
/// scene went.
///
/// # Errors
///
/// The disk refused.
pub fn trash(path: &Path) -> std::io::Result<PathBuf> {
    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    let bin = folder.join(TRASH_DIR);
    std::fs::create_dir_all(&bin)?;
    let stem = path
        .file_stem()
        .map_or_else(|| "scene".into(), |s| s.to_string_lossy().into_owned());
    let target = (0..10_000)
        .map(|n| {
            if n == 0 {
                bin.join(format!("{stem}.ron"))
            } else {
                bin.join(format!("{stem}-{n}.ron"))
            }
        })
        .find(|p| !p.exists() && !thumbnail_path(p).exists())
        .ok_or_else(|| std::io::Error::other(format!("{} is full", bin.display())))?;
    std::fs::rename(path, &target)?;
    let sidecar = thumbnail_path(path);
    if sidecar.is_file() {
        std::fs::rename(sidecar, thumbnail_path(&target))?;
    }
    Ok(target)
}

/// A scene path parameter: it must end in `.ron`.
fn scene_path(text: &str) -> Result<PathBuf, OpError> {
    let path = PathBuf::from(text);
    if crate::ops::has_extension(&path, "ron") {
        Ok(path)
    } else {
        Err(OpError::Invalid(format!(
            "a scene file ends in .ron: `{text}`"
        )))
    }
}

/// Show a folder in the Scenes panel.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BrowseDir {
    /// The folder (relative to the working directory). Empty: a dialog.
    pub path: String,
}

impl Operation for BrowseDir {
    const META: OpMeta = OpMeta {
        id: "scene.browse_dir",
        lisp: "browse-dir",
        doc: "Show the scene files in a folder in the Scenes panel (\"\" = a folder dialog). Workstation config: not saved with the scene, no undo step.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"path": "scenes"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let folder = (!self.path.is_empty()).then(|| PathBuf::from(&self.path));
        Ok(OpOutput::none().message(SceneFileRequest::Browse(folder)))
    }
}

/// Rename a scene file and its thumbnail.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RenameScene {
    /// The scene file now (`.ron`).
    pub from: String,
    /// Its new path (`.ron`); must not exist.
    pub to: String,
}

impl Operation for RenameScene {
    const META: OpMeta = OpMeta {
        id: "scene.rename",
        lisp: "rename-scene",
        doc: "Rename the scene file from to the path to (both .ron), with its thumbnail. Refuses to replace a file. A file operation: no undo step.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"from": "target/old.ron", "to": "target/new.ron"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let from = scene_path(&self.from)?;
        let to = scene_path(&self.to)?;
        Ok(OpOutput::none()
            .message(SceneFileRequest::Rename { from, to })
            .returning(self.to))
    }
}

/// Move a scene file and its thumbnail to the trash folder beside it.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TrashScene {
    /// The scene file (`.ron`).
    pub path: String,
}

impl Operation for TrashScene {
    const META: OpMeta = OpMeta {
        id: "scene.trash",
        lisp: "trash-scene",
        doc: "Move the scene file at path (.ron) and its thumbnail into the .trash folder beside it. A file operation: no undo step; move it back by hand.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"path": "target/unwanted.ron"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(SceneFileRequest::Trash(scene_path(&self.path)?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gradiance-browser-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        dir
    }

    #[test]
    fn the_listing_pairs_each_scene_with_its_sidecar() {
        let dir = temp("list");
        for f in ["b.ron", "a.ron", "a.png", "notes.txt"] {
            std::fs::write(dir.join(f), "").unwrap_or_else(|e| panic!("{e}"));
        }
        let list = list_scenes(&dir);
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(list[0].thumbnail, Some(dir.join("a.png")));
        assert_eq!(list[1].thumbnail, None);
    }

    #[test]
    fn rename_moves_the_sidecar_and_refuses_to_replace() {
        let dir = temp("rename");
        for f in ["a.ron", "a.png", "c.ron"] {
            std::fs::write(dir.join(f), "").unwrap_or_else(|e| panic!("{e}"));
        }
        assert!(rename(&dir.join("a.ron"), &dir.join("c.ron")).is_err());
        rename(&dir.join("a.ron"), &dir.join("b.ron")).unwrap_or_else(|e| panic!("{e}"));
        assert!(dir.join("b.ron").is_file() && dir.join("b.png").is_file());
        assert!(!dir.join("a.ron").exists() && !dir.join("a.png").exists());
    }

    #[test]
    fn trash_moves_both_files_and_numbers_a_second_copy() {
        let dir = temp("trash");
        for _ in 0..2 {
            std::fs::write(dir.join("a.ron"), "").unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(dir.join("a.png"), "").unwrap_or_else(|e| panic!("{e}"));
            trash(&dir.join("a.ron")).unwrap_or_else(|e| panic!("{e}"));
        }
        let bin = dir.join(TRASH_DIR);
        assert!(bin.join("a.ron").is_file() && bin.join("a.png").is_file());
        assert!(bin.join("a-1.ron").is_file() && bin.join("a-1.png").is_file());
        assert_eq!(list_scenes(&dir), Vec::<SceneEntry>::new());
    }
}
