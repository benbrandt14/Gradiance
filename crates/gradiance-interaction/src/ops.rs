//! The operations this layer owns (`gradiance-op`): the camera-feel settings
//! read and write. `CameraSettings` is initialised here, so this layer
//! publishes its mirror and declares its verbs.
//!
//! `camera.set` is **workstation config** (CLAUDE.md invariant 4): it writes
//! the settings resource through the config seam and records no undo step,
//! so reverting a scene edit can never move someone's camera feel.

use bevy::prelude::*;
use gradiance_domain::settings::CameraSettings;
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
