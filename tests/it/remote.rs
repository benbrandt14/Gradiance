//! The remote control surface (`script::remote`), driven in-process: the
//! methods are called through Bevy's own `RemoteMethods` table — the one the
//! HTTP transport dispatches into — without opening a socket.

use crate::harness::{body_count, headless_app_with, step, undo};
use bevy::prelude::*;
use bevy::remote::{BrpResult, RemoteMethodSystemId, RemoteMethods, error_codes};
use gradiance::command::HistoryInfo;
use gradiance::prelude::GameState;
use gradiance::script::bridge::ScriptLog;
use gradiance::script::remote::{KEPT_BUILTINS, RemoteControlPlugin, method};
use serde_json::{Value, json};

/// A paused headless app with the remote surface on (no HTTP transport).
pub(crate) fn remote_app() -> App {
    let mut app = headless_app_with(|app| {
        app.add_plugins(RemoteControlPlugin { port: None });
    });
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.update();
    app
}

/// Calls one method as the BRP server would; `None` if it is not served.
pub(crate) fn call(app: &mut App, name: &str, params: Value) -> Option<BrpResult> {
    let id = match app.world().resource::<RemoteMethods>().get(name)? {
        RemoteMethodSystemId::Instant(id) => *id,
        RemoteMethodSystemId::Watching(_) => return None,
    };
    let result = app.world_mut().run_system_with(id, Some(params)).ok()?;
    Some(result)
}

fn ok(app: &mut App, name: &str, params: Value) -> Value {
    let result = call(app, name, params);
    assert!(matches!(result, Some(Ok(_))), "{name} failed: {result:?}");
    result.and_then(Result::ok).unwrap_or_default()
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

#[test]
fn lists_the_operation_catalog() {
    let mut app = remote_app();
    let ops = ok(&mut app, method::OPS, Value::Null);
    let spawn_box = ops
        .as_array()
        .and_then(|ops| ops.iter().find(|op| op["name"] == "spawn-box"))
        .cloned()
        .unwrap_or_else(|| panic!("spawn-box not listed: {ops}"));
    assert_eq!(spawn_box["signature"], "(spawn-box x y w h)");
    assert_eq!(spawn_box["category"], "edit");
    assert_eq!(spawn_box["args"], 4);
    assert!(spawn_box["doc"].as_str().is_some_and(|d| !d.is_empty()));
}

#[test]
fn a_name_call_is_one_operation_one_body_one_undo_step() {
    let mut app = remote_app();
    let before = undo_depth(&app);
    let handle = ok(
        &mut app,
        method::OP,
        json!({"name": "spawn-box", "args": [0, 200, 40, 20.5]}),
    );
    // The reply is the verb's value (the new body's handle). The intent is
    // queued; the next frame's dispatcher commits it, like a console line.
    assert!(handle.as_str().is_some_and(|h| h.len() == 36), "{handle}");
    app.update();
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(undo_depth(&app), before + 1);
    // A follow-up read sees the write.
    // (Query verbs answer in floats, as in the console.)
    let count = ok(&mut app, method::QUERY, json!({"name": "body-count"}));
    assert!(
        count.as_f64().is_some_and(|n| (n - 1.0).abs() < 1e-9),
        "{count}"
    );
    // A registry verb goes through the registry, not a script run: no
    // Scheme text, no console line (brief U-2b).
    assert!(app.world().resource::<ScriptLog>().0.is_empty());

    app.update();
    undo(&mut app);
    assert_eq!(body_count(&mut app), 0);
}

/// A query of something that is not there answers `null` over remote (NaN
/// in Lisp).
#[test]
fn a_query_of_a_missing_body_answers_null() {
    let mut app = remote_app();
    let x = ok(
        &mut app,
        method::QUERY,
        json!({"name": "body-x", "args": [3]}),
    );
    assert_eq!(x, Value::Null);
    let x = ok(
        &mut app,
        method::QUERY,
        json!({"id": "body.x", "params": {"i": 3}}),
    );
    assert_eq!(x, Value::Null);
}

/// A registry error names the operation once, in both forms (U-2a found
/// `group.create: group.create: …`).
#[test]
fn a_registry_error_names_the_operation_once() {
    let mut app = remote_app();
    let err = call(
        &mut app,
        method::OP,
        json!({"name": "spawn-box", "args": [1]}),
    )
    .and_then(Result::err)
    .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
    assert_eq!(err.message, "spawn-box: expected 4 arguments, got 1");

    let err = call(
        &mut app,
        method::OP,
        json!({"id": "group.create", "params": {"targets": "selection"}}),
    )
    .and_then(Result::err)
    .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
    assert!(
        err.message.starts_with("group.create: invalid type"),
        "{}",
        err.message
    );
    assert_eq!(err.message.matches("group.create").count(), 1);
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn an_unknown_op_is_an_invalid_params_error() {
    let mut app = remote_app();
    let err = call(&mut app, method::OP, json!({"name": "no-such-op"}))
        .and_then(Result::err)
        .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
    assert!(err.message.contains("no-such-op"), "{}", err.message);
}

/// The remote surface is registry-only (brief U-3b): a VM built-in exists
/// only inside a script run, so naming one is refused, and nothing runs.
#[test]
fn a_vm_builtin_is_refused() {
    let mut app = remote_app();
    for name in ["tool-x", "action-x", "ops", "describe"] {
        let err = call(&mut app, method::OP, json!({"name": name, "args": []}))
            .and_then(Result::err)
            .unwrap_or_else(|| panic!("{name}: expected an error"));
        assert_eq!(err.code, error_codes::INVALID_PARAMS, "{name}");
        assert!(
            err.message.contains(name) && err.message.contains("only inside a script run"),
            "{name}: {}",
            err.message
        );
    }
    assert!(app.world().resource::<ScriptLog>().0.is_empty(), "no run");
}

/// The panel operations (brief U-3b) answer remotely by id and by name.
#[test]
fn panel_operations_answer_by_id_and_by_name() {
    let mut app = remote_app();
    ok(
        &mut app,
        method::OP,
        json!({"id": "panel.hide", "params": {"name": "properties"}}),
    );
    app.update();
    let open = ok(
        &mut app,
        method::QUERY,
        json!({"name": "panel-open?", "args": ["properties"]}),
    );
    assert_eq!(open, json!(false));

    ok(
        &mut app,
        method::OP,
        json!({"id": "panel.toggle", "params": {"name": "properties"}}),
    );
    app.update();
    let open = ok(
        &mut app,
        method::QUERY,
        json!({"id": "panel.open", "params": {"name": "properties"}}),
    );
    assert_eq!(open, json!(true), "toggle flips a closed panel open");

    let panels = ok(&mut app, method::QUERY, json!({"name": "panels"}));
    let rows = panels.as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 14, "{panels}");
    assert_eq!(
        rows[0],
        json!({"name": "outliner", "open": rows[0]["open"]})
    );
    assert!(
        rows.contains(&json!({"name": "properties", "open": true})),
        "{panels}"
    );
    // Panel writes are editor state: no undo step.
    assert_eq!(undo_depth(&app), 0);
    // And a panel write is not a query.
    let err = call(
        &mut app,
        method::QUERY,
        json!({"name": "panel-show", "args": ["properties"]}),
    )
    .and_then(Result::err)
    .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
}

/// An unknown panel name is refused at the call, naming the known panels.
#[test]
fn an_unknown_panel_name_is_refused() {
    let mut app = remote_app();
    let err = call(
        &mut app,
        method::OP,
        json!({"id": "panel.show", "params": {"name": "nope"}}),
    )
    .and_then(Result::err)
    .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
    assert!(
        err.message.starts_with("panel.show: no panel named `nope`")
            && err.message.contains("outliner, groups, properties"),
        "{}",
        err.message
    );
}

#[test]
fn an_argument_of_the_wrong_type_is_refused_before_anything_runs() {
    let mut app = remote_app();
    let err = call(
        &mut app,
        method::OP,
        json!({"name": "spawn-box", "args": [0, 0, {"w": 1}, 1]}),
    )
    .and_then(Result::err)
    .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
    assert!(app.world().resource::<ScriptLog>().0.is_empty());
}

#[test]
fn query_refuses_a_write() {
    let mut app = remote_app();
    let err = call(
        &mut app,
        method::QUERY,
        json!({"name": "spawn-box", "args": [0, 0, 10, 10]}),
    )
    .and_then(Result::err)
    .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn raw_world_mutation_is_not_served() {
    let mut app = remote_app();
    let methods = app.world().resource::<RemoteMethods>();
    for write in [
        "world.spawn_entity",
        "world.insert_components",
        "world.mutate_components",
        "world.remove_components",
        "world.despawn_entity",
        "world.reparent_entities",
        "world.insert_resources",
        "world.mutate_resources",
        "world.remove_resources",
        "world.trigger_event",
        "world.write_message",
    ] {
        assert!(methods.get(write).is_none(), "{write} is served");
    }
    let mut served = methods.methods();
    served.sort();
    let mut expected: Vec<String> = [method::OPS, method::OP, method::QUERY]
        .iter()
        .chain(KEPT_BUILTINS)
        .map(|s| (*s).to_owned())
        .collect();
    expected.sort();
    assert_eq!(served, expected);
    // `rpc.discover` reports the curated table, not Bevy's default one.
    let discovered = ok(&mut app, "rpc.discover", Value::Null).to_string();
    assert!(discovered.contains(method::OP));
    assert!(!discovered.contains("world.insert_components"));
}

#[test]
fn a_remote_spawn_simulates_like_a_console_spawn() {
    // While the simulation runs, a remotely spawned body must get its
    // collider before physics steps it (the run's intents are dispatched in
    // `Update`, not where BRP handles the request).
    let mut app = headless_app_with(|app| {
        app.add_plugins(RemoteControlPlugin { port: None });
    });
    ok(
        &mut app,
        method::OP,
        json!({"name": "spawn-box", "args": [0, 0, 40, 40]}),
    );
    step(&mut app, 10);
    let mut bodies = app
        .world_mut()
        .query_filtered::<&Transform, With<gradiance::domain::Body>>();
    let ys: Vec<f32> = bodies.iter(app.world()).map(|t| t.translation.y).collect();
    assert_eq!(ys.len(), 1);
    // Finite, and falling under gravity.
    assert!(ys[0].is_finite() && ys[0] < 0.0, "{ys:?}");
}
