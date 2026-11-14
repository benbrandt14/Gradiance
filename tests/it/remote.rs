//! The remote control surface (`script::remote`), driven in-process: the
//! methods are called through Bevy's own `RemoteMethods` table — the one the
//! HTTP transport dispatches into — without opening a socket.

use crate::harness::{body_count, headless_app_with, undo};
use bevy::prelude::*;
use bevy::remote::{BrpResult, RemoteMethodSystemId, RemoteMethods, error_codes};
use gradiance::command::HistoryInfo;
use gradiance::prelude::GameState;
use gradiance::script::bridge::ScriptLog;
use gradiance::script::remote::{KEPT_BUILTINS, RemoteControlPlugin, SCRIPT_ERROR, method};
use serde_json::{Value, json};

/// A paused headless app with the remote surface on (no HTTP transport).
fn remote_app() -> App {
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
fn call(app: &mut App, name: &str, params: Value) -> Option<BrpResult> {
    let id = match app.world().resource::<RemoteMethods>().get(name)? {
        RemoteMethodSystemId::Instant(id) => *id,
        RemoteMethodSystemId::Watching(_) => return None,
    };
    let result = app.world_mut().run_system_with(id, Some(params)).ok()?;
    Some(result)
}

fn ok(app: &mut App, name: &str, params: Value) -> Value {
    match call(app, name, params) {
        Some(Ok(value)) => value,
        other => panic!("{name} failed: {other:?}"),
    }
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
fn an_op_call_is_one_run_one_body_one_undo_step() {
    let mut app = remote_app();
    let before = undo_depth(&app);
    let handle = ok(
        &mut app,
        method::OP,
        json!({"name": "spawn-box", "args": [0, 200, 40, 20.5]}),
    );
    // The reply is the verb's value (the new body's handle), and the body is
    // already committed — the call dispatched its intents before replying.
    assert!(handle.as_str().is_some_and(|h| h.len() == 36), "{handle}");
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(undo_depth(&app), before + 1);
    // A follow-up read sees the write.
    // (Query verbs answer in floats, as in the console.)
    let count = ok(&mut app, method::QUERY, json!({"name": "body-count"}));
    assert!(
        count.as_f64().is_some_and(|n| (n - 1.0).abs() < 1e-9),
        "{count}"
    );
    // It shows in the console like any other run.
    let log = &app.world().resource::<ScriptLog>().0;
    assert!(
        log.iter()
            .any(|e| e.ok && e.input == "(spawn-box 0 200 40 20.5)")
    );

    app.update();
    undo(&mut app);
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

#[test]
fn a_failing_run_returns_the_consoles_message() {
    let mut app = remote_app();
    // Wrong arity: steel refuses the call; nothing is spawned.
    let err = call(
        &mut app,
        method::OP,
        json!({"name": "spawn-box", "args": [1]}),
    )
    .and_then(Result::err)
    .unwrap_or_else(|| panic!("expected an error"));
    assert_eq!(err.code, SCRIPT_ERROR);
    let log = &app.world().resource::<ScriptLog>().0;
    let entry = log.last().unwrap_or_else(|| panic!("no log entry"));
    assert!(!entry.ok);
    assert_eq!(err.message, entry.output, "the console's text, verbatim");
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn an_argument_with_no_scheme_form_is_refused_before_running() {
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
