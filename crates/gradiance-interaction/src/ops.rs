//! The operations this layer owns (`gradiance-op`): the camera-feel settings
//! read and write, the chrome preset (brief U-7) and the unit display
//! (brief 5-3). `CameraSettings`, `ChromeSettings` and `UnitSettings` are
//! initialised here, so this layer publishes their mirrors and declares the
//! verbs.
//!
//! `camera.set`, `chrome.apply` and `units.display` are **workstation config** (CLAUDE.md
//! invariant 4): they write the settings resource through the config seam
//! and record no undo step, so reverting a scene edit can never move
//! someone's camera feel or restyle their panels.

use bevy::prelude::*;
use gradiance_domain::settings::{CameraSettings, Chrome, ChromeSettings, UnitSettings};
use gradiance_op::reflect_path::read_path;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam,
};
use gradiance_units::{AngleUnit, LengthUnit, UnitPrefs};
use serde::Deserialize;

/// Registers this layer's operations and the camera mirror.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<CameraGet>();
        registry.register::<CameraSet>();
        registry.register::<ChromeApply>();
        registry.register::<ChromeList>();
        registry.register::<UnitsDisplay>();
        registry.register::<UnitsShown>();
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
    if let Some(units) = world.get_resource::<UnitSettings>() {
        ctx.insert(*units);
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

/// Show quantities in the editor's fields with these units (brief 5-3).
/// Display only: values stay SI. Workstation config: no undo step, not
/// saved with the scene.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UnitsDisplay {
    /// How angles show: `deg` or `rad`.
    pub angle: String,
    /// How lengths show: `m`, `cm` or `mm`.
    pub length: String,
}

impl Operation for UnitsDisplay {
    const META: OpMeta = OpMeta {
        id: "units.display",
        lisp: "display-units",
        doc: "Show angles in deg or rad and lengths in m, cm or mm in every field (display only; workstation config, never undone).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"angle": "rad", "length": "cm"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let angle = AngleUnit::named(&self.angle).ok_or_else(|| {
            OpError::Invalid(format!("no angle unit `{}` (deg, rad)", self.angle))
        })?;
        let length = LengthUnit::named(&self.length).ok_or_else(|| {
            OpError::Invalid(format!("no length unit `{}` (m, cm, mm)", self.length))
        })?;
        Ok(OpOutput::none().replace_setting(UnitSettings {
            prefs: UnitPrefs { angle, length },
        }))
    }
}

/// The unit display in use: `[angle, length]`, e.g. `["deg", "m"]`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UnitsShown {}

impl Operation for UnitsShown {
    const META: OpMeta = OpMeta {
        id: "units.shown",
        lisp: "units-shown",
        doc: "The units the fields show, as (angle length), e.g. (\"deg\" \"m\").",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let prefs = ctx.get::<UnitSettings>()?.prefs;
        Ok(OpOutput::none().returning(vec![prefs.angle.name(), prefs.length.name()]))
    }
}
