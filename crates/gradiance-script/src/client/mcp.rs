//! `gradiance mcp`: the Model Context Protocol on stdio, over the remote
//! surface of a running app.
//!
//! Newline-delimited JSON-RPC 2.0 (MCP's stdio transport). The subset served:
//! `initialize`, `ping`, `tools/list`, `tools/call`; a notification gets no
//! reply. Four tools, not one per operation (brief U-6, decision 5):
//!
//! - `ops` — the catalog: id, Lisp name, doc, category, fields, params
//!   schema and example of each callable operation (optional `prefix`).
//! - `query` — one `query` operation (read-only; maps to `gradiance.query`).
//! - `execute` — any one operation (`gradiance.op`).
//! - `batch` — several operations in order; stops at the first error. Each
//!   call is its own undo step, as successive remote calls are.
//!
//! A call names its operation by id with named `params`
//! (`{"op": "body.spawn_box", "params": {"x": 0, …}}`) or by Lisp name with
//! positional `args` (`{"op": "spawn-box", "args": [0, …]}`). A refused
//! operation is a tool result with `isError: true` (MCP's rule: the model
//! sees it), not a protocol error.

use super::cli::callable;
use super::{CallError, Remote};
use crate::remote::method;
use serde_json::{Map, Value, json};
use std::io::{self, BufRead, Write};

/// The protocol version answered when the client asks for one this server
/// does not know.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Versions whose subset above is the same.
const KNOWN_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", PROTOCOL_VERSION];

/// The usage text.
pub const USAGE: &str = "\
usage: gradiance mcp [--launch] [--port N]
  Serves the Model Context Protocol on stdio for the Gradiance app whose
  remote surface listens on 127.0.0.1:PORT (start it with `gradiance --remote`).
  --launch   start `gradiance --remote PORT` if nothing answers; stop it on exit
  --port N   default: $GRADIANCE_PORT, else 15702
  Tools: ops, query, execute, batch.";

/// What the server tells the model at `initialize`.
const INSTRUCTIONS: &str = "Drives a running Gradiance physics sandbox through its operation registry, \
the same operations its menus, keys and Lisp console use. Call `ops` first (optionally with a \
prefix such as `body.` or `ui.`): each row gives the id, the params schema and an example. \
`execute` runs one operation by id with named params, or by Lisp name with positional args. \
`query` runs read-only operations only. `batch` runs several in order and stops at the first error. \
Edits are undoable (`edit.undo`) and take effect in the next frame, so read state with a later call. \
`ui.inspect` lists widget names that `ui.click`, `ui.drag` and `ui.key` target; `ui.screenshot` \
writes a PNG to a path.";

/// Serves MCP: one JSON-RPC message per line of `input`, one reply per line
/// of `output`, until `input` ends.
pub fn serve(
    remote: &mut impl Remote,
    input: impl BufRead,
    output: &mut impl Write,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(remote, message),
            Err(err) => Some(error(&Value::Null, -32700, &format!("parse error: {err}"))),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// The reply to one message; `None` for a notification (no `id`).
pub fn handle(remote: &mut impl Remote, message: Value) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match message["method"].as_str().unwrap_or("") {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(remote, &params),
        "" => Err((-32600, "a request needs a `method`".to_owned())),
        other => Err((-32601, format!("method `{other}` is not served"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => error(&id, code, &message),
    })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn initialize(params: &Value) -> Value {
    let asked = params["protocolVersion"].as_str().unwrap_or("");
    let version = if KNOWN_VERSIONS.contains(&asked) {
        asked
    } else {
        PROTOCOL_VERSION
    };
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {"name": "gradiance", "version": env!("CARGO_PKG_VERSION")},
        "instructions": INSTRUCTIONS,
    })
}

/// The schema of one call: an operation and its params or args.
fn call_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "op": {"type": "string", "description": "Operation id (`body.spawn_box`) or Lisp name (`spawn-box`)."},
            "params": {"type": "object", "description": "Named params, with an id. See the `ops` schema and example."},
            "args": {"type": "array", "description": "Positional args in declared field order, with a Lisp name."}
        },
        "required": ["op"],
        "additionalProperties": false
    })
}

/// The four tools.
pub fn tools() -> Value {
    json!([
        {
            "name": "ops",
            "title": "List operations",
            "description": "List the operations the app serves: id, Lisp name, doc, category (edit, config, query, editor), fields, params JSON schema and an example. Filter with a prefix of the id or name.",
            "inputSchema": {
                "type": "object",
                "properties": {"prefix": {"type": "string", "description": "Keep rows whose id or name starts with this."}},
                "additionalProperties": false
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "query",
            "title": "Run a query operation",
            "description": "Run one read-only (`query`) operation and return its value. Any other operation is refused.",
            "inputSchema": call_schema(),
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "execute",
            "title": "Run an operation",
            "description": "Run one operation and return its value (for a spawn, the new object's id). Edits are undoable and land in the next frame.",
            "inputSchema": call_schema(),
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "openWorldHint": false}
        },
        {
            "name": "batch",
            "title": "Run operations in order",
            "description": "Run several operations in order; stop at the first error. Returns each value. Each call is its own undo step, and each reads the state of the frame it runs in.",
            "inputSchema": {
                "type": "object",
                "properties": {"calls": {"type": "array", "items": call_schema()}},
                "required": ["calls"],
                "additionalProperties": false
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "openWorldHint": false}
        }
    ])
}

/// `tools/call`. A tool that fails is a result with `isError`; only a
/// malformed request is a protocol error.
fn call_tool(remote: &mut impl Remote, params: &Value) -> Result<Value, (i64, String)> {
    let name = params["name"]
        .as_str()
        .ok_or((-32602, "`name` must be a tool name".to_owned()))?;
    let args = match &params["arguments"] {
        Value::Null => Value::Object(Map::new()),
        object @ Value::Object(_) => object.clone(),
        _ => return Err((-32602, "`arguments` must be an object".to_owned())),
    };
    let outcome = match name {
        "ops" => list(remote, &args),
        "query" => single(remote, method::QUERY, &args),
        "execute" => single(remote, method::OP, &args),
        "batch" => return Ok(batch(remote, &args)),
        other => return Err((-32602, format!("no tool `{other}`"))),
    };
    Ok(match outcome {
        Ok(value) => result(&value, false),
        Err(why) => result(&Value::String(why), true),
    })
}

/// A tool result: the value as JSON text.
fn result(value: &Value, is_error: bool) -> Value {
    let text = match value {
        Value::String(text) if is_error => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    json!({"content": [{"type": "text", "text": text}], "isError": is_error})
}

fn list(remote: &mut impl Remote, args: &Value) -> Result<Value, String> {
    let prefix = args["prefix"].as_str().unwrap_or("");
    let catalog = remote
        .call(method::OPS, Value::Null)
        .map_err(|e| e.to_string())?;
    Ok(Value::Array(
        callable(&catalog)
            .filter(|row| {
                row["id"].as_str().is_some_and(|id| id.starts_with(prefix))
                    || row["name"].as_str().is_some_and(|n| n.starts_with(prefix))
            })
            .map(|row| {
                json!({
                    "id": row["id"], "name": row["name"], "doc": row["doc"],
                    "category": row["category"], "fields": row["fields"],
                    "schema": row["schema"], "example": row["example"],
                })
            })
            .collect(),
    ))
}

fn single(remote: &mut impl Remote, method: &str, call: &Value) -> Result<Value, String> {
    let request = request(call)?;
    remote.call(method, request).map_err(|err| match err {
        CallError::Rpc { message, .. } => message,
        other => other.to_string(),
    })
}

/// The remote params of one call: the `id` form for an id (it has a dot),
/// the `name` form for a Lisp name.
pub fn request(call: &Value) -> Result<Value, String> {
    let op = call["op"]
        .as_str()
        .ok_or("`op` must be an operation id or Lisp name")?;
    let params = &call["params"];
    let args = &call["args"];
    if op.contains('.') {
        if !args.is_null() {
            return Err(format!(
                "`{op}` is an id: give named `params` (`args` go with a Lisp name)"
            ));
        }
        Ok(json!({"id": op, "params": if params.is_null() { json!({}) } else { params.clone() }}))
    } else {
        if !params.is_null() {
            return Err(format!(
                "`{op}` is a Lisp name: give positional `args`, or call its id with `params` (see `ops`)"
            ));
        }
        Ok(json!({"name": op, "args": if args.is_null() { json!([]) } else { args.clone() }}))
    }
}

fn batch(remote: &mut impl Remote, args: &Value) -> Value {
    let Some(calls) = args["calls"].as_array() else {
        return result(&Value::String("`calls` must be an array".into()), true);
    };
    let mut values = Vec::with_capacity(calls.len());
    for (i, call) in calls.iter().enumerate() {
        match single(remote, method::OP, call) {
            Ok(value) => values.push(value),
            Err(why) => {
                let text = format!(
                    "call {i} ({}) failed: {why}\ncompleted before it: {}",
                    call["op"].as_str().unwrap_or("?"),
                    serde_json::to_string(&values).unwrap_or_default()
                );
                return result(&Value::String(text), true);
            }
        }
    }
    result(&Value::Array(values), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers the catalog and echoes a call's params.
    struct Fake;

    impl Remote for Fake {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
            match (method, params["id"].as_str().or(params["name"].as_str())) {
                (method::OPS, _) => Ok(json!([
                    {"id": "body.spawn_box", "name": "spawn-box", "doc": "Spawn a box.", "schema": {}},
                    {"id": null, "name": "tool-x"}
                ])),
                (_, Some("bad")) => Err(CallError::Rpc {
                    code: -32602,
                    message: "unknown operation `bad`".into(),
                }),
                _ => Ok(json!({"method": method, "params": params})),
            }
        }
    }

    fn rpc(message: Value) -> Value {
        handle(&mut Fake, message).unwrap_or(Value::Null)
    }

    fn tool(name: &str, arguments: Value) -> Value {
        rpc(json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}}))["result"]
            .clone()
    }

    fn text(result: &Value) -> Value {
        result["content"][0]["text"]
            .as_str()
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or(Value::Null)
    }

    #[test]
    fn initialize_echoes_a_known_version_and_lists_tools() {
        let reply = rpc(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26", "capabilities": {}}}));
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert!(reply["result"]["capabilities"]["tools"].is_object());
        let reply = rpc(json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": {"protocolVersion": "1999-01-01"}}));
        assert_eq!(reply["result"]["protocolVersion"], PROTOCOL_VERSION);
        let tools = rpc(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}));
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert_eq!(names, ["ops", "query", "execute", "batch"]);
    }

    #[test]
    fn notifications_get_no_reply_and_unknown_methods_an_error() {
        assert_eq!(
            handle(
                &mut Fake,
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            ),
            None
        );
        let reply = rpc(json!({"jsonrpc": "2.0", "id": 4, "method": "resources/list"}));
        assert_eq!(reply["error"]["code"], -32601);
        assert_eq!(
            rpc(json!({"jsonrpc": "2.0", "id": 5, "method": "ping"}))["result"],
            json!({})
        );
    }

    #[test]
    fn ops_hides_vm_builtins() {
        let rows = text(&tool("ops", json!({})));
        assert_eq!(rows.as_array().map(Vec::len), Some(1));
        assert_eq!(rows[0]["id"], "body.spawn_box");
        assert_eq!(text(&tool("ops", json!({"prefix": "ui."}))), json!([]));
    }

    #[test]
    fn execute_uses_the_id_or_the_name_form() {
        let by_id = text(&tool(
            "execute",
            json!({"op": "body.spawn_box", "params": {"x": 1}}),
        ));
        assert_eq!(by_id["method"], method::OP);
        assert_eq!(
            by_id["params"],
            json!({"id": "body.spawn_box", "params": {"x": 1}})
        );
        let by_name = text(&tool("query", json!({"op": "body-count"})));
        assert_eq!(by_name["method"], method::QUERY);
        assert_eq!(by_name["params"], json!({"name": "body-count", "args": []}));
        let mixed = tool("execute", json!({"op": "spawn-box", "params": {}}));
        assert_eq!(mixed["isError"], true);
    }

    #[test]
    fn a_refused_operation_is_a_tool_error_not_a_protocol_error() {
        let refused = tool("execute", json!({"op": "bad"}));
        assert_eq!(refused["isError"], true);
        assert_eq!(refused["content"][0]["text"], "unknown operation `bad`");
    }

    #[test]
    fn batch_stops_at_the_first_error() {
        let ok = tool(
            "batch",
            json!({"calls": [{"op": "a.b"}, {"op": "c-d", "args": [1]}]}),
        );
        assert_eq!(ok["isError"], false);
        assert_eq!(text(&ok).as_array().map(Vec::len), Some(2));
        let stopped = tool(
            "batch",
            json!({"calls": [{"op": "a.b"}, {"op": "bad"}, {"op": "c.d"}]}),
        );
        assert_eq!(stopped["isError"], true);
        let why = stopped["content"][0]["text"].as_str().unwrap_or("");
        assert!(why.starts_with("call 1 (bad) failed"), "{why}");
    }

    #[test]
    fn serve_answers_line_by_line() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\nnot json\n\
            {\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
        let mut out = Vec::new();
        serve(&mut Fake, &input[..], &mut out).unwrap_or_else(|e| panic!("{e}"));
        let lines: Vec<Value> = String::from_utf8_lossy(&out)
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["id"], 1);
        assert_eq!(lines[1]["error"]["code"], -32700);
    }
}
