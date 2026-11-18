//! The operations this layer owns (`gradiance-op`): the camera-feel settings
//! read and write, and the chrome preset (brief U-7). `CameraSettings` and
//! `ChromeSettings` are initialised here, so this layer publishes the camera
//! mirror and declares the verbs.
//!
//! `camera.set` and `chrome.apply` are **workstation config** (CLAUDE.md
//! invariant 4): they write the settings resource through the config seam
//! and record no undo step, so reverting a scene edit can never move
//! someone's camera feel or restyle their panels.

use bevy::prelude::*;
use gradiance_domain::settings::{CameraSettings, Chrome, ChromeSettings};
use gradiance_op::reflect_path::read_path;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam,
};
use serde::Deserialize;

/// Registers this layer's operations and the camera mirror.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<CameraGet>();
        registry.register::<CameraSet>();
        registry.register::<ChromeApply>();
        registry.register::<ChromeList>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_camera_mirror);
}

/// Publishes the camera settings into a new operation context.
fn fill_camera_mirror(world: &mut World, ctx: &mut OpContext) {
    if let Some(settings) = world.get_resource::<CameraSettings>() {
        ctx.insert(settings.clone());
    }
}

/// Read one camera-feel setting by reflect-path.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CameraGet {
    /// The reflect-path of a scalar in `CameraSettings`.
    pub path: String,
}

impl Operation for CameraGet {
    const META: OpMeta = OpMeta {
        id: "camera.get",
        lisp: "camera-get",
        doc: "Read a camera-feel setting: \"zoom_step\", \"pan_speed\", \"orbit_speed_deg\"; NaN if the path does not resolve.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"path": "zoom_step"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<CameraSettings>()?;
        let value = read_path(settings, &self.path).unwrap_or(f64::NAN);
        Ok(OpOutput::none().returning(value))
    }
}

/// Set one camera-feel setting by reflect-path. Workstation config: no undo
/// step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CameraSet {
    /// The reflect-path of a scalar in `CameraSettings`.
    pub path: String,
    /// The new value.
    pub value: f64,
}

impl Operation for CameraSet {
    const META: OpMeta = OpMeta {
        id: "camera.set",
        lisp: "camera-set",
        doc: "Set a camera-feel setting by reflect-path (workstation config, never undone).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"path": "pan_speed", "value": 900}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let settings = ctx.get::<CameraSettings>()?;
        Ok(OpOutput::none().setting(settings, &self.path, self.value)?)
    }
}

/// Every chrome preset's name, comma-separated (for error messages).
fn chrome_names() -> String {
    Chrome::ALL.map(Chrome::name).join(", ")
}

/// Switch the editor chrome to a named preset (brief U-7). Workstation
/// config: no undo step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ChromeApply {
    /// The preset's name (`chrome.list`).
    pub name: String,
}

impl Operation for ChromeApply {
    const META: OpMeta = OpMeta {
        id: "chrome.apply",
        lisp: "chrome-apply",
        doc: "Draw the editor panels with a chrome preset (blender, blender-blue, stock); workstation config, never undone.",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "blender"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let chrome = Chrome::named(&self.name).ok_or_else(|| {
            OpError::Invalid(format!(
                "no chrome preset `{}` (presets: {})",
                self.name,
                chrome_names()
            ))
        })?;
        Ok(OpOutput::none().replace_setting(ChromeSettings { chrome }))
    }
}

/// Every chrome preset's name, in menu order.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ChromeList {}

impl Operation for ChromeList {
    const META: OpMeta = OpMeta {
        id: "chrome.list",
        lisp: "chromes",
        doc: "Every chrome preset's name, in menu order (the first is the default).",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().returning(Chrome::ALL.map(Chrome::name).to_vec()))
    }
}
