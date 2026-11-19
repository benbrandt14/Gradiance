//! The scene look (brief 4b-3): apply a named preset, and set one render,
//! lighting or scenery value by reflect-path.
//!
//! All four are the config seam on scene-content resources, so a settled
//! change is one undo step (`commit_settings_edits`); applying a look
//! replaces three resources in one frame, which is still one step.

use crate::intent::name;
use bevy::prelude::*;
use gradiance_domain::settings::{LightingSettings, RenderSettings, ScenerySettings};
use gradiance_domain::theme::Look;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<LookApply>();
    registry.register::<LookList>();
    registry.register::<RenderSet>();
    registry.register::<LightingSet>();
    registry.register::<ScenerySet>();
}

/// The look's names, comma-separated (for error messages).
fn look_names() -> String {
    Look::ALL.map(Look::name).join(", ")
}

/// Apply a named look: its palette tone, ink, render, lighting and scenery
/// presets, in one undo step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LookApply {
    /// The look's name (`look.list`).
    pub name: String,
}

impl Operation for LookApply {
    const META: OpMeta = OpMeta {
        id: "look.apply",
        lisp: "look-apply",
        doc: "Apply a scene look by name (studio, blueprint, poster): palette tone, overlay ink, render, lighting and scenery presets — one undo step.",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(name::SETTINGS),
        example: r#"{"name": "blueprint"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let look = Look::named(&self.name).ok_or_else(|| {
            OpError::Invalid(format!("no look `{}` (looks: {})", self.name, look_names()))
        })?;
        Ok(OpOutput::none()
            .replace_setting(look.render())
            .replace_setting(look.lighting())
            .replace_setting(look.scenery()))
    }
}

/// Every look's name, in menu order.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LookList {}

impl Operation for LookList {
    const META: OpMeta = OpMeta {
        id: "look.list",
        lisp: "looks",
        doc: "Every scene look's name, in menu order (the first is the default).",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().returning(Look::ALL.map(Look::name).to_vec()))
    }
}

/// Set one render setting by reflect-path (`"bands"`, `"white_point"`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RenderSet {
    /// The reflect-path of a scalar in `RenderSettings`.
    pub path: String,
    /// The new value.
    pub value: f64,
}

impl Operation for RenderSet {
    const META: OpMeta = OpMeta {
        id: "render.set",
        lisp: "render-set",
        doc: "Set a render (toon) setting by reflect-path, e.g. \"bands\", \"rim_strength\", \"white_point\" (a settled edit is one undo step).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(name::SETTINGS),
        example: r#"{"path": "bands", "value": 3}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<RenderSettings>()?;
        Ok(OpOutput::none().setting(settings, &self.path, self.value)?)
    }
}

/// Set one lighting setting by reflect-path (`"ambient"`,
/// `"lights[0].elevation_deg"`, `"ssao"`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LightingSet {
    /// The reflect-path of a scalar in `LightingSettings`.
    pub path: String,
    /// The new value.
    pub value: f64,
}

impl Operation for LightingSet {
    const META: OpMeta = OpMeta {
        id: "lighting.set",
        lisp: "lighting-set",
        doc: "Set a lighting setting by reflect-path, e.g. \"ambient\", \"lights[0].elevation_deg\", \"ssao\" (a settled edit is one undo step).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(name::SETTINGS),
        example: r#"{"path": "lights[0].elevation_deg", "value": 30}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<LightingSettings>()?;
        Ok(OpOutput::none().setting(settings, &self.path, self.value)?)
    }
}

/// Set one scenery setting by reflect-path (`"perspective_deg"`,
/// `"back_offset"`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ScenerySet {
    /// The reflect-path of a scalar in `ScenerySettings`.
    pub path: String,
    /// The new value.
    pub value: f64,
}

impl Operation for ScenerySet {
    const META: OpMeta = OpMeta {
        id: "scenery.set",
        lisp: "scenery-set",
        doc: "Set a scenery setting by reflect-path, e.g. \"perspective_deg\", \"back_offset\", \"back_visible\" (a settled edit is one undo step).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(name::SETTINGS),
        example: r#"{"path": "perspective_deg", "value": 20}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<ScenerySettings>()?;
        Ok(OpOutput::none().setting(settings, &self.path, self.value)?)
    }
}
