//! The client doorways (`script::client`, brief U-6): the CLI parser and the
//! MCP server against the live catalog, in-process through the same method
//! table the HTTP transport dispatches into, plus one real HTTP round trip.

use crate::harness::{body_count, headless_app_with};
use crate::remote::{call, remote_app};
use bevy::prelude::*;
use gradiance::prelude::GameState;
use gradiance::script::client::cli::{callable, params_from_args};
use gradiance::script::client::{CallError, HttpRemote, Remote, mcp};
use gradiance::script::remote::{RemoteControlPlugin, method};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// The remote surface of an in-process app, as a [`Remote`].
struct InProcess<'a>(&'a mut App);

impl Remote for InProcess<'_> {
    fn call(&mut self, name: &str, params: Value) -> Result<Value, CallError> {
        match call(self.0, name, params) {
            Some(Ok(value)) => Ok(value),
            Some(Err(err)) => Err(CallError::Rpc {
                code: i64::from(err.code),
                message: err.message,
            }),
            None => Err(CallError::Protocol(format!("`{name}` is not served"))),
        }
    }
}

fn catalog(app: &mut App) -> Value {
    match call(app, method::OPS, Value::Null) {
        Some(Ok(catalog)) => catalog,
        other => panic!("gradiance.ops failed: {other:?}"),
    }
}

/// The JSON type name of `value`, in JSON Schema words.
fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Schema round trip (`docs/unified-surface-plan.md` § 3.5 test 2): every
/// catalog row's schema has one property per declared field, and each
/// example value has its field's schema type.
#[test]
fn every_schema_names_the_fields_and_types_the_example() {
    let mut app = remote_app();
    let catalog = catalog(&mut app);
    let mut checked = 0;
    for row in callable(&catalog) {
        let id = row["id"].as_str().unwrap_or("?");
        let fields: Vec<&str> = row["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let properties = row["schema"]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{id}: no schema properties: {row}"));
        let keys: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|f| properties.contains_key(*f))
            .collect();
        assert_eq!(keys, fields, "{id}: schema properties differ from fields");
        assert_eq!(properties.len(), fields.len(), "{id}: extra properties");
        let example = row["example"]
            .as_object()
            .unwrap_or_else(|| panic!("{id}: example is not an object: {row}"));
        for (field, value) in example {
            let Some(kind) = properties[field]["type"].as_str() else {
                continue;
            };
            let actual = json_type(value);
            let fits = actual == kind || (kind == "number" && actual == "integer");
            assert!(fits, "{id}.{field}: example {value} is not a {kind}");
        }
        checked += 1;
    }
    assert!(checked > 50, "only {checked} operations listed");
}

/// Doorway equivalence for the CLI parser (§ 3.5 test 1): every operation's
/// example, written as `--field value` flags the way a person types them,
/// parses back to the example against the live catalog.
#[test]
fn every_example_round_trips_through_cli_flags() {
    let mut app = remote_app();
    let catalog = catalog(&mut app);
    for row in callable(&catalog) {
        let id = row["id"].as_str().unwrap_or("?");
        let Some(example) = row["example"].as_object() else {
            continue;
        };
        let mut args = Vec::new();
        for (field, value) in example {
            args.push(format!("--{field}"));
            args.push(match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            });
        }
        let parsed = params_from_args(row, &args);
        assert_eq!(
            parsed.as_ref(),
            Ok(&row["example"]),
            "{id}: `{}` parses differently",
            args.join(" ")
        );
    }
}

/// MCP `execute` and `query` against the live registry: one spawn, one undo
/// step, and a read that sees it.
#[test]
fn mcp_execute_spawns_and_query_reads() {
    let mut app = remote_app();
    let reply = mcp::handle(
        &mut InProcess(&mut app),
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
            "name": "execute",
            "arguments": {"op": "body.spawn_box", "params": {"x": 0, "y": 2, "w": 1, "h": 1}}
        }}),
    )
    .unwrap_or_default();
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    app.update();
    assert_eq!(body_count(&mut app), 1);

    let reply = mcp::handle(
        &mut InProcess(&mut app),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "query", "arguments": {"op": "body-count"}
        }}),
    )
    .unwrap_or_default();
    let count: f64 = reply["result"]["content"][0]["text"]
        .as_str()
        .and_then(|t| t.parse().ok())
        .unwrap_or(f64::NAN);
    assert!((count - 1.0).abs() < 1e-9, "{reply}");

    // `query` refuses a write; the refusal reaches the model as a tool error.
    let reply = mcp::handle(
        &mut InProcess(&mut app),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "query", "arguments": {"op": "spawn-box", "args": [0, 2, 1, 1]}
        }}),
    )
    .unwrap_or_default();
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    app.update();
    assert_eq!(body_count(&mut app), 1);
}

/// MCP `ops` lists registry operations with their schema, and no VM
/// built-in.
#[test]
fn mcp_ops_lists_schemas_without_builtins() {
    let mut app = remote_app();
    let reply = mcp::handle(
        &mut InProcess(&mut app),
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
            "name": "ops", "arguments": {"prefix": "body.spawn_box"}
        }}),
    )
    .unwrap_or_default();
    let rows: Value = reply["result"]["content"][0]["text"]
        .as_str()
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();
    assert_eq!(rows[0]["id"], "body.spawn_box", "{reply}");
    assert_eq!(rows[0]["schema"]["required"], json!(["x", "y", "w", "h"]));
    let all = mcp::handle(
        &mut InProcess(&mut app),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "ops", "arguments": {}
        }}),
    )
    .unwrap_or_default();
    let text = all["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(!text.contains("\"tool-x\""), "a VM built-in is listed");
}

/// The hand-written HTTP client against Bevy's real transport: a query and
/// an edit over a socket, while the app runs frames.
#[test]
fn the_http_client_talks_to_the_real_transport() {
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map_or_else(|e| panic!("no free port: {e}"), |a| a.port());
    let mut app = headless_app_with(|app| {
        app.add_plugins(RemoteControlPlugin { port: Some(port) });
    });
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.update();

    let client = std::thread::spawn(move || -> Vec<Result<Value, String>> {
        let mut remote = HttpRemote::new(port);
        let deadline = Instant::now() + Duration::from_secs(20);
        // The server task starts on the first frames: retry until it answers.
        let first = loop {
            match remote.call(method::QUERY, json!({"name": "body-count"})) {
                Err(CallError::Unreachable { .. }) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                other => break other,
            }
        };
        let spawn = remote.call(
            method::OP,
            json!({"id": "body.spawn_box", "params": {"x": 0, "y": 2, "w": 1, "h": 1}}),
        );
        let refused = remote.call(method::OP, json!({"id": "no.such_op"}));
        [first, spawn, refused]
            .into_iter()
            .map(|r| r.map_err(|e| e.to_string()))
            .collect()
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    while !client.is_finished() && Instant::now() < deadline {
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        client.is_finished(),
        "the HTTP client did not finish in 30 s"
    );
    let replies = client.join().unwrap_or_default();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0].as_ref().ok().and_then(Value::as_f64), Some(0.0));
    assert!(
        replies[1]
            .as_ref()
            .ok()
            .and_then(Value::as_str)
            .is_some_and(|h| h.len() == 36),
        "{:?}",
        replies[1]
    );
    assert!(
        replies[2]
            .as_ref()
            .err()
            .is_some_and(|e| e.contains("unknown operation")),
        "{:?}",
        replies[2]
    );
    app.update();
    assert_eq!(body_count(&mut app), 1);
}
