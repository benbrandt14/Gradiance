//! The scene-file operations (brief R-1): `scene.save`, `scene.load` and
//! `scene.dump`. They register here because this layer owns the disk: the
//! save and load operations lower to the [`SaveSceneRequest`] and
//! [`LoadSceneRequest`] that File ▸ Save / Open and Ctrl+S / Ctrl+O write, so
//! the menu, the keys, a script and a remote client take one path.
//!
//! `scene.dump` returns the [`SceneRecord`] a save writes, as JSON: the serde
//! projection of the same record, not a second format (what goes on disk
//! stays RON). The record ([`SceneDump`]) is a **gated** mirror
//! (`OpContextFillers::add_for`): captured only for a context that may lower
//! `scene.dump` (that call, or a script run whose source names
//! `scene-dump`), once per context, so a run still reads one snapshot
//! (scripting rule 3) and no other call or F3 frame pays for it.
//!
//! # Insert, copy and paste (brief R-3)
//!
//! The same RON text is the unit of all three. `scene.insert` adds a file's
//! bodies, joints and groups to the scene; `edit.copy` writes the
//! selection's part of the [`SceneDump`] as scene text (to the clipboard,
//! through [`ClipboardWrite`], which the UI layer drains); `edit.paste`
//! adds such a text. Insert and paste lower to one
//! [`gradiance_command::intent::InsertIntent`] whose records
//! already carry fresh ids from the context
//! ([`gradiance_command::insert_cmd::fresh_copy`]), so one
//! call is one undo step and the caller gets the new bodies' ids back.

use crate::{LoadSceneRequest, SaveSceneRequest};
use bevy::prelude::*;
use gradiance_command::insert_cmd::{fragment, fragment_anchor, fresh_copy};
use gradiance_command::intent::{InsertIntent, name};
use gradiance_command::snapshot::SceneSnapshot;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target,
    Value,
};
use gradiance_scene::{SceneRecord, from_ron, to_ron};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The committed scene as a save would write it: the mirror `scene.dump`
/// reads.
#[derive(Debug, Clone)]
pub struct SceneDump(pub SceneRecord);

/// The operations that read the [`SceneDump`] mirror.
const DUMP_OPS: &[&str] = &[DumpScene::META.id, CopyScene::META.id];

/// Text for the system clipboard: what `edit.copy` writes. The UI layer
/// drains it (the clipboard is egui's); headless, nothing does.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ClipboardWrite(pub String);

/// Request to pick a scene file to insert (`scene.insert` with an empty
/// path): the dialog's answer is invoked as `scene.insert` with that path.
#[derive(Message, Debug, Clone)]
pub struct InsertSceneRequest {
    /// Where the fragment goes (`[]`: where it was saved).
    pub at: Vec<f32>,
}

/// Registers the scene-file operations and the record mirror.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<SaveScene>();
        registry.register::<LoadScene>();
        registry.register::<DumpScene>();
        registry.register::<InsertScene>();
        registry.register::<CopyScene>();
        registry.register::<PasteScene>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add_for(DUMP_OPS, fill_scene_dump);
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

/// The place `at` names: `[]` none (the fragment stays where it was saved),
/// `[x, y]` a world point in metres.
fn place(at: &[f32]) -> Result<Option<Vec2>, OpError> {
    match *at {
        [] => Ok(None),
        [x, y] if x.is_finite() && y.is_finite() => Ok(Some(Vec2::new(x, y))),
        _ => Err(OpError::Invalid(format!(
            "`at` is [] (where it was saved) or [x, y], not {at:?}"
        ))),
    }
}

/// The output of an insert or a paste: `scene`'s objects with fresh ids
/// from the context, its anchor ([`fragment_anchor`]) moved to `at`, as one
/// [`InsertIntent`]. Returns the new bodies' ids.
fn insert_output(scene: &SceneRecord, at: &[f32], ctx: &OpContext) -> Result<OpOutput, OpError> {
    let anchor = fragment_anchor(scene)
        .ok_or_else(|| OpError::Invalid("the scene text holds no body or node".to_owned()))?;
    let offset = place(at)?.map_or(Vec2::ZERO, |to| to - anchor);
    let copy = fresh_copy(scene, offset, || ctx.new_id());
    let ids: Vec<Value> = copy
        .bodies
        .iter()
        .map(|b| Value::from(b.id.0.to_string()))
        .collect();
    Ok(OpOutput::none()
        .message(InsertIntent { scene: copy })
        .returning(Value::Array(ids)))
}

/// Add a scene file's bodies, joints and groups to the scene.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct InsertScene {
    /// The file (relative to the working directory). Empty: a file dialog.
    pub path: String,
    /// `[]`: where the file has them; `[x, y]`: centred there.
    pub at: Vec<f32>,
}

impl Operation for InsertScene {
    const META: OpMeta = OpMeta {
        id: "scene.insert",
        lisp: "insert-scene",
        doc: "Add a .ron file's bodies, joints, nodes and groups to the scene with fresh ids, one undo step (\"\" = a dialog). `at`: [] keeps the file's positions, [x y] centres the bodies there. Returns the new bodies' ids.",
        seam: Seam::Edit,
        menu: Some("File/Insert…"),
        chord: None,
        undo: Some(name::INSERT),
        example: r#"{"path": "tests/fixtures/scenes/pendulum.ron", "at": [6, 2]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        place(&self.at)?;
        if self.path.is_empty() {
            return Ok(OpOutput::none().message(InsertSceneRequest { at: self.at }));
        }
        let scene = std::fs::read_to_string(&self.path)
            .map_err(|e| e.to_string())
            .and_then(|text| from_ron(&text).map_err(|e| e.to_string()))
            .map_err(|why| OpError::Invalid(format!("`{}`: {why}", self.path)))?;
        insert_output(&scene, &self.at, ctx)
    }
}

/// Copy bodies (with the joints between them, their nodes and their
/// groups) as scene text.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CopyScene {
    /// The bodies (`"selection"`: every selected body and node).
    pub targets: Vec<Target>,
}

impl Operation for CopyScene {
    const META: OpMeta = OpMeta {
        id: "edit.copy",
        lisp: "copy-ron",
        doc: "Copy bodies, the joints between them, their nodes and their groups to the clipboard as scene RON text (what a save writes, cut to them); returns the text (\"\" when nothing is named, and the clipboard is left alone). No undo step.",
        seam: Seam::Editor,
        menu: Some("Edit/Copy"),
        chord: Some("Ctrl+C"),
        undo: None,
        example: r#"{"targets": [3, 4]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let snapshot = ctx.get::<SceneSnapshot>()?;
        let mut objects = gradiance_command::ops::body_set(ctx, snapshot, &self.targets)?;
        // A selected free node goes too (a body's nodes go with it).
        if self.targets.contains(&Target::Selection) {
            objects.extend(
                ctx.selection()?
                    .objects
                    .iter()
                    .filter(|id| snapshot.nodes.contains(id)),
            );
        }
        let dump = ctx.get::<SceneDump>()?;
        // An empty selection copies nothing and leaves the clipboard alone,
        // as Delete with nothing selected deletes nothing.
        let Some(part) = fragment(&dump.0, &objects) else {
            return Ok(OpOutput::none().returning(""));
        };
        let text = to_ron(&part)
            .map_err(|e| OpError::Invalid(format!("the copy does not encode: {e}")))?;
        Ok(OpOutput::none()
            .message(ClipboardWrite(text.clone()))
            .returning(text))
    }
}

/// Add scene text (a copy, or a whole scene file's text) to the scene.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PasteScene {
    /// Scene RON text, as `edit.copy` writes it or a `.ron` file holds it.
    pub text: String,
    /// `[]`: where it was copied; `[x, y]`: centred there.
    pub at: Vec<f32>,
}

impl Operation for PasteScene {
    const META: OpMeta = OpMeta {
        id: "edit.paste",
        lisp: "paste-ron",
        doc: "Add scene RON text (from edit.copy or a .ron file) to the scene with fresh ids, one undo step. `at`: [] keeps its positions, [x y] centres the bodies there (Ctrl+V: the cursor). Returns the new bodies' ids.",
        seam: Seam::Edit,
        menu: Some("Edit/Paste"),
        chord: Some("Ctrl+V"),
        undo: Some(name::INSERT),
        example: PASTE_EXAMPLE,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = from_ron(&self.text)
            .map_err(|e| OpError::Invalid(format!("the text is not scene RON: {e}")))?;
        insert_output(&scene, &self.at, ctx)
    }
}

/// `edit.paste`'s example: one 1 m box, as `edit.copy` writes it less the
/// fields that default when absent (physics, appearance, depth, groups,
/// nodes, the environment's settings).
const PASTE_EXAMPLE: &str = r#"{"text": "(version: 8, app_version: \"\", bodies: [(id: (\"00000000-0000-0000-0000-00000000000a\"), pose: (pos: (0.0, 1.0), rot: 0.0), shape: Box(width: 1.0, height: 1.0))], joints: [], environment: ())", "at": [2, 3]}"#;

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_op::OpIdSource;

    fn ctx() -> OpContext {
        OpContext::new(OpIdSource::Random)
    }

    #[test]
    fn the_paste_example_is_scene_text_with_one_body() {
        let params: Value = serde_json::from_str(PasteScene::META.example).unwrap_or_default();
        let text = params["text"].as_str().unwrap_or_default();
        let scene = from_ron(text).map_err(|e| e.to_string());
        assert_eq!(scene.map(|s| s.bodies.len()), Ok(1));
    }

    #[test]
    fn at_is_empty_or_a_point() {
        assert!(matches!(place(&[]), Ok(None)));
        assert!(matches!(place(&[1.0, 2.0]), Ok(Some(_))));
        assert!(place(&[1.0]).is_err());
        assert!(place(&[f32::NAN, 0.0]).is_err());
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
