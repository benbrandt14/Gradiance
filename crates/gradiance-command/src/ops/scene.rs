//! Scene-wide operations: the history steps, a new empty scene, a count, and
//! the simulation settings write.

use crate::intent::name;
use crate::intent::{AdjustLastIntent, LoadSceneIntent, RedoIntent, UndoIntent};
use crate::snapshot::SceneSnapshot;
use bevy::prelude::*;
use gradiance_domain::settings::SimSettings;
use gradiance_op::{
    LastOp, OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Value, json,
};
use gradiance_scene::{EnvironmentRecord, FORMAT_VERSION, SceneRecord};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Undo>();
    registry.register::<Redo>();
    registry.register::<NewScene>();
    registry.register::<BodyCount>();
    registry.register::<SimSet>();
    registry.register::<AdjustLast>();
    registry.register::<LastOpQuery>();
}

/// Undo the last command.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Undo {}

impl Operation for Undo {
    const META: OpMeta = OpMeta {
        id: "edit.undo",
        lisp: "undo",
        doc: "Undo the last command — the same step Edit ▸ Undo takes.",
        seam: Seam::Edit,
        menu: Some("Edit/Undo"),
        chord: Some("Ctrl+Z"),
        // Undo moves a step to the redo stack; it records none.
        undo: None,
        example: "{}",
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(UndoIntent))
    }
}

/// Redo the last undone command.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Redo {}

impl Operation for Redo {
    const META: OpMeta = OpMeta {
        id: "edit.redo",
        lisp: "redo",
        doc: "Redo the last undone command — the same step Edit ▸ Redo takes.",
        seam: Seam::Edit,
        menu: Some("Edit/Redo"),
        chord: Some("Ctrl+Shift+Z"),
        // Redo moves a step back from the redo stack; it records none.
        undo: None,
        example: "{}",
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(RedoIntent))
    }
}

/// Replace the scene with an empty one: one undo step, as File ▸ New scene
/// takes. The scene-content settings (gravity, render style, …) return to
/// their defaults; the workstation config (grid, snap, camera feel, saved
/// workspaces) is the person's and stays as it is (brief Q-3).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct NewScene {}

impl Operation for NewScene {
    const META: OpMeta = OpMeta {
        id: "scene.new",
        lisp: "new-scene",
        doc: "Replace the scene with an empty one (default gravity, look and signals; your grid, snap, camera and workspaces stay) — one undo step, as File ▸ New scene.",
        seam: Seam::Edit,
        menu: Some("File/New scene"),
        chord: None,
        undo: Some(name::LOAD_SCENE),
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let mut scene = empty_scene();
        let mut environment = ctx.get::<EnvironmentRecord>()?.clone();
        environment.set_scene_content(&scene.environment);
        scene.environment = environment;
        Ok(OpOutput::none().message(LoadSceneIntent { scene }))
    }
}

/// An empty scene at the current format version, with the default
/// environment.
pub fn empty_scene() -> SceneRecord {
    SceneRecord {
        version: FORMAT_VERSION,
        app_version: String::new(),
        bodies: Vec::new(),
        joints: Vec::new(),
        nodes: Vec::new(),
        environment: EnvironmentRecord::default(),
    }
}

/// How many authored bodies the committed scene holds.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodyCount {}

impl Operation for BodyCount {
    const META: OpMeta = OpMeta {
        id: "body.count",
        lisp: "body-count",
        doc: "Number of authored bodies in the committed scene.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        Ok(OpOutput::none().returning(scene.count()))
    }
}

/// Set one simulation setting by reflect-path (`"gravity.y"`, `"speed"`).
/// Scene content: a settled edit is one undo step through
/// `commit_settings_edits`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimSet {
    /// The reflect-path of a scalar in `SimSettings`.
    pub path: String,
    /// The new value.
    pub value: f64,
}

impl Operation for SimSet {
    const META: OpMeta = OpMeta {
        id: "sim.set",
        lisp: "sim-set",
        doc: "Set a simulation setting by reflect-path (config seam; a settled edit is one undo step).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(name::SETTINGS),
        example: r#"{"path": "speed", "value": 0.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<SimSettings>()?;
        Ok(OpOutput::none().setting(settings, &self.path, self.value)?)
    }
}

/// A new value for one field of the last operation's params: a number, a
/// flag or a text (an object id, a name). It takes the field's own kind
/// ([`gradiance_op::set_param`]).
#[derive(Debug, Clone, Deserialize, Reflect)]
#[serde(untagged)]
pub enum AdjustValue {
    /// A number (a whole-number field rounds it; a flag reads non-zero).
    Number(f64),
    /// A flag.
    Flag(bool),
    /// A text.
    Text(String),
}

impl AdjustValue {
    fn json(&self) -> Value {
        match self {
            Self::Number(n) => json!(n),
            Self::Flag(b) => json!(b),
            Self::Text(t) => json!(t),
        }
    }
}

/// Adjust Last Operation (brief U-11): set one field of the last
/// operation's params and run it again in place of its step, as one undo
/// step. `field` is a dotted path into the params (`"w"`, `"step.0"`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AdjustLast {
    /// The field: a param name, with `.i` for an element of a list.
    pub field: String,
    /// Its new value.
    pub value: AdjustValue,
}

impl Operation for AdjustLast {
    const META: OpMeta = OpMeta {
        id: "edit.adjust_last",
        lisp: "adjust-last",
        doc: "Set one param of the last operation (field \"w\", \"step.0\") and run it again in place of its undo step: still one step. Errors when the newest step is not an operation's.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        // It replaces the last operation's step with one of the same label;
        // it records none of its own.
        undo: None,
        example: r#"{"field": "r", "value": 0.75}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let Some(last) = ctx.get::<LastOp>()?.0.as_ref() else {
            return Err(OpError::Invalid(
                "there is no operation to adjust: the newest undo step is not an operation's"
                    .to_owned(),
            ));
        };
        let mut params = last.call.params.clone();
        gradiance_op::set_param(&mut params, &self.field, &self.value.json())?;
        Ok(OpOutput::none().message(AdjustLastIntent {
            id: last.call.id,
            params,
            step: last.step,
        }))
    }
}

/// The last operation (brief U-11): `{"id", "params"}` of the operation
/// that made the newest undo step, or nothing when the newest step is not
/// an operation's (a tool gesture, an undo, a run, a load).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LastOpQuery {}

impl Operation for LastOpQuery {
    const META: OpMeta = OpMeta {
        id: "edit.last_op",
        lisp: "last-op",
        doc: "The operation that made the newest undo step, as {id, params} (what Adjust Last Operation edits); null when there is none.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let value = ctx.get::<LastOp>()?.0.as_ref().map_or(
            Value::Null,
            |last| json!({"id": last.call.id, "params": last.call.params}),
        );
        Ok(OpOutput::none().returning(value))
    }
}
