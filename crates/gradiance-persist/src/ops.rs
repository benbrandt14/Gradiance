//! The scene-file operations (brief R-1): `scene.save`, `scene.load` and
//! `scene.dump`. They register here because this layer owns the disk: the
//! save and load operations lower to the [`SaveSceneRequest`] and
//! [`LoadSceneRequest`] that File ▸ Save / Open and Ctrl+S / Ctrl+O write, so
//! the menu, the keys, a script and a remote client take one path.
//!
//! `scene.dump` returns the [`SceneRecord`] a save writes, as JSON: the serde
//! projection of the same record, not a second format (what goes on disk
//! stays RON). Each operation context captures the record once
//! ([`SceneDump`]), so a script run reads one snapshot (scripting rule 3).

use crate::{LoadSceneRequest, SaveSceneRequest};
use bevy::prelude::*;
use gradiance_command::intent::name;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Value,
};
use gradiance_scene::{SceneRecord, from_ron};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The committed scene as a save would write it: the mirror `scene.dump`
/// reads.
#[derive(Debug, Clone)]
pub struct SceneDump(pub SceneRecord);

/// Registers the scene-file operations and the record mirror.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<SaveScene>();
        registry.register::<LoadScene>();
        registry.register::<DumpScene>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_scene_dump);
}

/// Captures the record a save would write into a new operation context.
fn fill_scene_dump(world: &mut World, ctx: &mut OpContext) {
    ctx.insert(SceneDump(SceneRecord::capture(world)));
}

/// Whether `path` names a file with extension `ext` (any case).
fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Write the scene to a RON file.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SaveScene {
    /// The file (must end in `.ron`; relative to the working directory).
    /// Empty: the last saved or opened path, else a file dialog.
    pub path: String,
}

impl Operation for SaveScene {
    const META: OpMeta = OpMeta {
        id: "scene.save",
        lisp: "save-scene",
        doc: "Write the scene to a .ron file (relative to the working directory; \"\" = the last path, else a dialog). Returns the path; the file is written this frame. No undo step.",
        seam: Seam::Editor,
        menu: Some("File/Save"),
        chord: Some("Ctrl+S"),
        undo: None,
        example: r#"{"path": "target/scene.ron"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        if self.path.is_empty() {
            return Ok(OpOutput::none().message(SaveSceneRequest { path: None }));
        }
        let path = PathBuf::from(&self.path);
        if !has_extension(&path, "ron") {
            return Err(OpError::Invalid(format!(
                "a scene file ends in .ron: `{}`",
                self.path
            )));
        }
        Ok(OpOutput::none()
            .message(SaveSceneRequest { path: Some(path) })
            .returning(self.path))
    }
}

/// Replace the scene with one read from a RON file.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LoadScene {
    /// The file (relative to the working directory). Empty: a file dialog.
    pub path: String,
}

impl Operation for LoadScene {
    const META: OpMeta = OpMeta {
        id: "scene.load",
        lisp: "load-scene",
        doc: "Replace the scene with a .ron file's (relative to the working directory; \"\" = a dialog): one undo step, as File ▸ Open. An unreadable file is an error and changes nothing.",
        seam: Seam::Edit,
        menu: Some("File/Open…"),
        chord: Some("Ctrl+O"),
        undo: Some(name::LOAD_SCENE),
        example: r#"{"path": "tests/fixtures/scenes/pendulum.ron"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        if self.path.is_empty() {
            return Ok(OpOutput::none().message(LoadSceneRequest { path: None }));
        }
        // Read once here so a caller hears why a file will not load; the
        // request reads it again where it is applied.
        let path = PathBuf::from(&self.path);
        std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| from_ron(&text).map_err(|e| e.to_string()))
            .map_err(|why| OpError::Invalid(format!("`{}`: {why}", self.path)))?;
        Ok(OpOutput::none()
            .message(LoadSceneRequest { path: Some(path) })
            .returning(self.path))
    }
}

/// The scene as JSON: the record a save writes.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct DumpScene {}

impl Operation for DumpScene {
    const META: OpMeta = OpMeta {
        id: "scene.dump",
        lisp: "scene-dump",
        doc: "The whole scene as JSON: the record a save writes (bodies with their groups and depth, joints with their anchors, nodes, and the environment). Lisp gets the JSON text.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let dump = ctx.get::<SceneDump>()?;
        let value: Value = serde_json::to_value(&dump.0)
            .map_err(|e| OpError::Invalid(format!("the scene does not encode as JSON: {e}")))?;
        Ok(OpOutput::none().returning(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_op::OpIdSource;

    fn ctx() -> OpContext {
        OpContext::new(OpIdSource::Random)
    }

    #[test]
    fn a_save_needs_a_ron_path() {
        let err = SaveScene {
            path: "scene.txt".to_owned(),
        }
        .lower(&ctx())
        .err()
        .map(|e| e.to_string());
        assert!(err.is_some_and(|text| text.contains(".ron")));
        let ok = SaveScene {
            path: "out/Scene.RON".to_owned(),
        }
        .lower(&ctx());
        assert!(ok.is_ok_and(|out| out.value() == Some(&Value::from("out/Scene.RON"))));
    }

    #[test]
    fn an_empty_path_asks_and_a_missing_file_is_an_error() {
        let ask = LoadScene {
            path: String::new(),
        }
        .lower(&ctx());
        assert!(ask.is_ok_and(|out| !out.writes_nothing()));
        let missing = LoadScene {
            path: "no/such/scene.ron".to_owned(),
        }
        .lower(&ctx());
        assert!(missing.is_err());
    }
}
