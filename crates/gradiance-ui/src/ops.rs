//! The operations this layer owns (`gradiance-op`): the window's own reads
//! and requests (plan § 8, tier T2) — `ui.inspect` and `ui.screenshot`
//! (brief U-3), and the input a person gives — `ui.click`, `ui.drag`,
//! `ui.key`, `ui.text`, `ui.wheel` (brief U-4, Craft protocol ids). Being
//! registry operations, Lisp, the remote surface and the menus get them with
//! no code of their own.
//!
//! They register whether or not a window exists, so a headless run lists
//! them too: `ui.inspect` then answers an empty list, `ui.screenshot` logs
//! that there is no window, and the input operations still write their
//! messages (keys and buttons reach `ButtonInput`; there is no cursor).
//!
//! An input operation's target (`at`, `from`, `to`) is a widget name, which
//! resolves through the [`WidgetSnapshot`] mirror to its handle (else its
//! rect's centre) as drawn last frame, or a window point `[x, y]` in egui
//! logical points. The gesture plays over the next frames
//! ([`crate::host::input`]); the operation returns at once.

use crate::host::input::{MAX_STEPS, UiInput, parse_chord};
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
        registry.register::<UiClick>();
        registry.register::<UiDrag>();
        registry.register::<UiKey>();
        registry.register::<UiText>();
        registry.register::<UiWheel>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_widget_mirror);
    app.init_resource::<WidgetSnapshot>();
    app.init_resource::<ShotQueue>();
    app.add_message::<ScreenshotRequest>();
    app.init_resource::<crate::host::input::InputQueue>();
    app.add_message::<UiInput>();
    // What winit would write; headless there is no `WindowPlugin` to add them.
    app.add_message::<bevy::window::CursorMoved>();
    app.add_message::<bevy::window::WindowEvent>();
    app.add_systems(
        PreUpdate,
        crate::host::input::play_input.before(bevy::input::InputSystems),
    );
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

/// Where an input operation acts: a widget name (`"tools:Box"`, resolved
/// through the widget mirror) or a window point `[x, y]` in logical points.
#[derive(Debug, Clone, Deserialize, Reflect)]
#[serde(untagged)]
pub enum Target {
    /// A name `ui.inspect` lists.
    Name(String),
    /// A window point.
    Point([f32; 2]),
}

impl Target {
    /// The window point this target names, as drawn last frame.
    fn resolve(&self, ctx: &OpContext) -> Result<Vec2, OpError> {
        match self {
            Self::Point([x, y]) => {
                let at = Vec2::new(*x, *y);
                if at.is_finite() {
                    Ok(at)
                } else {
                    Err(OpError::Invalid(format!("[{x}, {y}] is not a point")))
                }
            }
            Self::Name(name) => {
                let snapshot = ctx.get::<WidgetSnapshot>()?;
                let widget = snapshot.0.iter().find(|w| &w.name == name);
                widget
                    .and_then(|w| {
                        w.at.map(Vec2::from).or(w
                            .rect
                            .map(|[x0, y0, x1, y1]| Vec2::new(x0 + x1, y0 + y1) / 2.0))
                    })
                    .ok_or_else(|| {
                        OpError::Invalid(format!(
                            "no widget `{name}` was drawn last frame (see ui.inspect)"
                        ))
                    })
            }
        }
    }
}

/// Lowers one gesture: it is queued, and the operation returns nothing.
fn inject(gesture: UiInput) -> OpOutput {
    OpOutput::none().message(gesture)
}

/// Click a widget or a window point.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiClick {
    /// A widget name or `[x, y]`.
    pub at: Target,
}

impl Operation for UiClick {
    const META: OpMeta = OpMeta {
        id: "ui.click",
        lisp: "ui-click",
        doc: "Click (left button) at a widget name from ui.inspect or a window point [x y]: move, press, release, one frame each, as real input.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"at": [640, 370]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(inject(UiInput::Click {
            at: self.at.resolve(ctx)?,
        }))
    }
}

/// Drag from one widget or point to another.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiDrag {
    /// Where the button goes down: a widget name or `[x, y]`.
    pub from: Target,
    /// Where it comes up.
    pub to: Target,
    /// Frames of movement between them (1–200).
    pub steps: u32,
}

impl Operation for UiDrag {
    const META: OpMeta = OpMeta {
        id: "ui.drag",
        lisp: "ui-drag",
        doc: "Drag (left button) from one widget name or point [x y] to another in steps frames (1-200): move, press, the steps, release, as real input; a tool commits one step on release.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"from": [640, 370], "to": [740, 370], "steps": 10}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        if !(1..=MAX_STEPS).contains(&self.steps) {
            return Err(OpError::Invalid(format!(
                "steps {} is not in 1..={MAX_STEPS}",
                self.steps
            )));
        }
        Ok(inject(UiInput::Drag {
            from: self.from.resolve(ctx)?,
            to: self.to.resolve(ctx)?,
            steps: self.steps,
        }))
    }
}

/// Press and release a key chord.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiKey {
    /// The chord, as the registry writes one: `Ctrl+Z`, `Delete`, `F1`.
    pub key: String,
}

impl Operation for UiKey {
    const META: OpMeta = OpMeta {
        id: "ui.key",
        lisp: "ui-key",
        doc: "Press and release a key chord, written as menus show it (Ctrl+Z, Ctrl+Shift+G, Delete, Escape, F1, A): modifiers down, key down, key up, modifiers up, one frame each.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"key": "Escape"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let chord = parse_chord(&self.key).map_err(OpError::Invalid)?;
        Ok(inject(UiInput::Key(chord)))
    }
}

/// Type text into whatever has keyboard focus.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiText {
    /// The text; a newline presses Enter.
    pub text: String,
}

impl Operation for UiText {
    const META: OpMeta = OpMeta {
        id: "ui.text",
        lisp: "ui-text",
        doc: "Type text into the focused field (click it first) in one frame; a newline presses Enter. Typed text trips no key shortcut.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"text": "1.5"}"#,
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(inject(UiInput::Text(self.text)))
    }
}

/// Turn the mouse wheel over a widget or point.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiWheel {
    /// A widget name or `[x, y]`.
    pub at: Target,
    /// Wheel lines; more than 0 turns it up (away from you).
    pub lines: f32,
}

impl Operation for UiWheel {
    const META: OpMeta = OpMeta {
        id: "ui.wheel",
        lisp: "ui-wheel",
        doc: "Move to a widget name or point [x y], then turn the wheel by lines (> 0: up, away from you), as real input.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"at": [640, 370], "lines": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        if !self.lines.is_finite() {
            return Err(OpError::Invalid(format!(
                "{} is not a wheel turn",
                self.lines
            )));
        }
        Ok(inject(UiInput::Wheel {
            at: self.at.resolve(ctx)?,
            lines: self.lines,
        }))
    }
}
