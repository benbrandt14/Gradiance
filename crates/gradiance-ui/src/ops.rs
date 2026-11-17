//! The operations this layer owns (`gradiance-op`): `ui.inspect` and
//! `ui.screenshot`, the window's own reads and requests (plan § 8, tier T2).
//! Being registry operations, Lisp, the remote surface and the menus get
//! them with no code of their own.
//!
//! They register whether or not a window exists, so a headless run lists
//! them too: `ui.inspect` then answers an empty list and `ui.screenshot`
//! logs that there is no window.

use crate::host::{ScreenshotRequest, ShotQueue};
use crate::widget_registry::WidgetSnapshot;
use bevy::prelude::*;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Value,
    json,
};
use serde::Deserialize;

/// Registers this layer's operations, the widget mirror, the screenshot
/// request and the systems that queue and take it.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<UiInspect>();
        registry.register::<UiScreenshot>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_widget_mirror);
    app.init_resource::<WidgetSnapshot>();
    app.init_resource::<ShotQueue>();
    app.add_message::<ScreenshotRequest>();
    app.add_systems(
        Last,
        (
            crate::host::queue_screenshots,
            crate::host::take_screenshots,
        )
            .chain(),
    );
}

/// Publishes the last frame's widgets into a new operation context.
fn fill_widget_mirror(world: &mut World, ctx: &mut OpContext) {
    if let Some(snapshot) = world.get_resource::<WidgetSnapshot>() {
        ctx.insert(snapshot.clone());
    }
}

/// List the widgets drawn last frame.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiInspect {
    /// Only names that start with this (`""`: every widget).
    pub prefix: String,
}

impl Operation for UiInspect {
    const META: OpMeta = OpMeta {
        id: "ui.inspect",
        lisp: "ui-inspect",
        doc: "List the widgets drawn last frame whose name starts with prefix (\"\" for all): name, area, rect [x0 y0 x1 y1] and handle [x y] in logical points.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"prefix": ""}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let snapshot = ctx.get::<WidgetSnapshot>()?;
        let rows: Vec<Value> = snapshot
            .matching(&self.prefix)
            .map(|w| json!({"name": w.name, "area": w.area, "rect": w.rect, "at": w.at}))
            .collect();
        Ok(OpOutput::none().returning(Value::Array(rows)))
    }
}

/// Save a PNG of the window.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiScreenshot {
    /// Where the PNG goes: a path ending in `.png`.
    pub path: String,
}

impl Operation for UiScreenshot {
    const META: OpMeta = OpMeta {
        id: "ui.screenshot",
        lisp: "ui-screenshot",
        doc: "Save a PNG of the window (scene and panels) at path, once the view settles (150 ms and 3 frames at least); returns the path at once, the file follows.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"path": "screenshot.png"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let is_png = std::path::Path::new(&self.path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
        if !is_png {
            return Err(OpError::Invalid(format!(
                "`{}` is not a .png path",
                self.path
            )));
        }
        Ok(OpOutput::none()
            .message(ScreenshotRequest {
                path: self.path.clone(),
            })
            .returning(self.path))
    }
}
