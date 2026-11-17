//! Scene-wide operations: the history steps, a count, and the simulation
//! settings write.

use crate::intent::name;
use crate::intent::{RedoIntent, UndoIntent};
use crate::snapshot::SceneSnapshot;
use bevy::prelude::*;
use gradiance_domain::settings::SimSettings;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Undo>();
    registry.register::<Redo>();
    registry.register::<BodyCount>();
    registry.register::<SimSet>();
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
