//! The widget registry and the `ui.*` operations (brief U-3).
//!
//! Headless: no window and no egui pass, so these tests publish on the test
//! thread and run the pass's last system (`end_frame`) themselves, or draw
//! one leaf renderer under `egui_kittest`. What the real pass publishes and
//! the PNG `ui.screenshot` writes need a window (see the brief's Outcome).

use crate::harness::headless_app_with;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::remote::{BrpResult, RemoteMethodSystemId, RemoteMethods};
use bevy::render::view::screenshot::Screenshot;
use bevy_egui::egui;
use egui_kittest::Harness;
use gradiance::core::states::ToolState;
use gradiance::op::invoke;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::script::remote::{RemoteControlPlugin, method};
use gradiance::ui::host::ScreenshotRequest;
use gradiance::ui::toolbar::tools_palette_ui;
use gradiance::ui::widget_registry::{
    WidgetSnapshot, clear_handles, end_frame, handles, publish_rect, publish_target,
};
use serde_json::{Value, json};

fn remote_app() -> App {
    headless_app_with(|app| {
        app.add_plugins(RemoteControlPlugin { port: None });
    })
}

/// Calls one remote method as the BRP server would.
fn call(app: &mut App, name: &str, params: Value) -> Option<BrpResult> {
    let id = match app.world().resource::<RemoteMethods>().get(name)? {
        RemoteMethodSystemId::Instant(id) => *id,
        RemoteMethodSystemId::Watching(_) => return None,
    };
    app.world_mut().run_system_with(id, Some(params)).ok()
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
}

/// What a frame published reaches `ui.inspect` over the remote surface and
/// the Lisp verb: names, areas, rects and handles, filtered by prefix.
#[test]
fn inspect_lists_the_widgets_the_last_frame_published() {
    let mut app = remote_app();
    let empty = call(
        &mut app,
        method::OP,
        json!({"id": "ui.inspect", "params": {"prefix": ""}}),
    );
    assert!(
        matches!(&empty, Some(Ok(Value::Array(rows))) if rows.is_empty()),
        "no frame drawn yet: {empty:?}"
    );

    // One frame: the pass clears, panels publish, `end_frame` mirrors.
    clear_handles();
    publish_rect("tools", rect(8.0, 120.0, 60.0, 400.0));
    publish_target("tools:Box", rect(10.0, 130.0, 40.0, 160.0));
    publish_rect("right_dock", rect(900.0, 24.0, 1280.0, 720.0));
    app.world_mut()
        .run_system_once(end_frame)
        .unwrap_or_else(|e| panic!("end_frame: {e}"));

    let all = call(
        &mut app,
        method::OP,
        json!({"id": "ui.inspect", "params": {"prefix": ""}}),
    );
    let Some(Ok(Value::Array(rows))) = all else {
        panic!("ui.inspect failed: {all:?}");
    };
    let names: Vec<&str> = rows.iter().filter_map(|r| r["name"].as_str()).collect();
    assert_eq!(names, ["tools", "tools:Box", "right_dock"]);
    assert_eq!(
        rows[1],
        json!({"name": "tools:Box", "area": "tools", "rect": [10.0, 130.0, 40.0, 160.0], "at": [25.0, 145.0]})
    );
    assert_eq!(rows[2]["at"], Value::Null, "a plain rect has no handle");

    let tools = call(
        &mut app,
        method::OP,
        json!({"name": "ui-inspect", "args": ["tools:"]}),
    );
    assert!(
        matches!(&tools, Some(Ok(Value::Array(rows))) if rows.len() == 1),
        "prefix filter: {tools:?}"
    );

    // The Lisp verb answers the same rows (each as JSON text).
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit("(length (ui-inspect \"\"))");
    app.update();
    let log = app.world().resource::<ScriptLog>().0.last().cloned();
    assert!(
        log.as_ref().is_some_and(|e| e.ok && e.output.contains('3')),
        "{log:?}"
    );
}

/// The tool palette names every tool button `tools:<name>`, in a real egui
/// pass, so U-4 can click a tool by name.
#[test]
fn the_tool_palette_publishes_a_target_per_tool() {
    clear_handles();
    let mut harness = Harness::new_ui(|ui| {
        tools_palette_ui(ui, ToolState::Select, None, &[]);
    });
    harness.run();
    let names: Vec<String> = handles("tools:").into_iter().map(|(n, _)| n).collect();
    for tool in ["tools:Select", "tools:Box", "tools:Circle"] {
        assert!(names.iter().any(|n| n == tool), "{tool} missing: {names:?}");
    }
    let snapshot = WidgetSnapshot::collect();
    let boxed = snapshot.matching("tools:Box").next().cloned();
    let boxed = boxed.unwrap_or_else(|| panic!("no tools:Box row"));
    let ([x0, y0, x1, y1], [x, y]) = (boxed.rect.unwrap_or_default(), boxed.at.unwrap_or_default());
    assert!(x1 > x0 && y1 > y0, "a drawn button has area: {boxed:?}");
    assert!(
        (x0..=x1).contains(&x) && (y0..=y1).contains(&y),
        "the handle is inside"
    );
}

/// `ui.screenshot` checks its path, returns it, and queues one request.
/// Headless, the request is dropped (no window) and no capture is spawned.
#[test]
fn screenshot_queues_a_request_and_refuses_a_non_png_path() {
    let mut app = remote_app();
    let bad = invoke(
        app.world_mut(),
        "ui.screenshot",
        json!({"path": "shot.jpg"}),
    );
    assert!(
        bad.as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("not a .png path")),
        "{bad:?}"
    );
    assert!(
        app.world()
            .resource::<Messages<ScreenshotRequest>>()
            .is_empty()
    );

    let ok = invoke(
        app.world_mut(),
        "ui.screenshot",
        json!({"path": "out/shot.PNG"}),
    );
    assert_eq!(ok.ok().flatten(), Some(json!("out/shot.PNG")));
    assert_eq!(
        app.world().resource::<Messages<ScreenshotRequest>>().len(),
        1
    );
    app.update();
    let captures = app
        .world_mut()
        .query::<&Screenshot>()
        .iter(app.world())
        .count();
    assert_eq!(captures, 0, "no window: nothing to capture");
}
