//! `gradiance ops` and `gradiance op`: the command-line doorway.
//!
//! ```text
//! gradiance ops [PREFIX] [--json]
//! gradiance op <id|name> [ARG]… [--field value]… [--params JSON]
//! ```
//!
//! `op` reads the catalog row of the operation, builds a **named** params
//! object (positional args by declared field order, flags by name) and calls
//! the `id` form, so the app's one serde decoder decides what is accepted.
//! A flag's text is coerced by its field's schema type
//! ([`coerce`]): a `string` field keeps the raw text; any other field is
//! parsed as JSON, then as a comma list (`1,2` → `[1, 2]`), then kept as
//! text for a field the schema cannot name (a target: an id or
//! `selection`).

use super::{CallError, Remote};
use crate::remote::method;
use serde_json::{Map, Value};
use std::io::Write;

/// Exit code: the operation failed (the app refused it).
pub const EXIT_FAILED: i32 = 1;
/// Exit code: the command line is wrong.
pub const EXIT_USAGE: i32 = 2;
/// Exit code: no app answers, or its reply is not understood.
pub const EXIT_REMOTE: i32 = 3;

/// The usage text.
pub const USAGE: &str = "\
usage:
  gradiance ops [PREFIX] [--json]       list operations (id or name starting with PREFIX)
  gradiance op <id|name> [ARG]... [--FIELD VALUE]... [--params JSON]
                                        invoke one operation; prints its value as JSON
  gradiance mcp [--launch]              serve MCP on stdio (see `gradiance mcp --help`)
  gradiance                             start the app; `--remote [PORT]` serves the above
options:
  --port N    the app's remote port (default: $GRADIANCE_PORT, else 15702)
examples:
  gradiance op body.spawn_box --x 0 --y 1 --w 1.6 --h 1.2
  gradiance op spawn-box 0 1 1.6 1.2
  gradiance op ui.click --target tools:Box";

/// Runs `ops` or `op` (`command`) with `args`; returns the exit code.
pub fn run(
    command: &str,
    args: &[String],
    remote: &mut impl Remote,
    out: &mut impl Write,
    err: &mut impl Write,
) -> i32 {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        let _ = writeln!(out, "{USAGE}");
        return 0;
    }
    let result = match command {
        "ops" => list(args, remote, out),
        "op" => invoke(args, remote, out),
        other => Err(Failure::Usage(format!("unknown command `{other}`"))),
    };
    match result {
        Ok(()) => 0,
        Err(Failure::Usage(why)) => {
            let _ = writeln!(err, "gradiance {command}: {why}\n{USAGE}");
            EXIT_USAGE
        }
        Err(Failure::Call(call @ CallError::Rpc { .. })) => {
            let _ = writeln!(err, "gradiance {command}: {call}");
            EXIT_FAILED
        }
        Err(Failure::Call(call)) => {
            let _ = writeln!(err, "gradiance {command}: {call}");
            EXIT_REMOTE
        }
    }
}

/// Why a command did not finish.
enum Failure {
    Usage(String),
    Call(CallError),
}

impl From<CallError> for Failure {
    fn from(err: CallError) -> Self {
        Self::Call(err)
    }
}

/// The catalog rows a client can call: those with an `id` (VM built-ins
/// have none: they exist only inside a script run).
pub fn callable(catalog: &Value) -> impl Iterator<Item = &Value> {
    catalog
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["id"].is_string())
}

/// The row whose id or Lisp name is `op`.
pub fn find<'a>(catalog: &'a Value, op: &str) -> Option<&'a Value> {
    callable(catalog).find(|row| row["id"] == op || row["name"] == op)
}

/// `gradiance ops [PREFIX] [--json]`.
fn list(args: &[String], remote: &mut impl Remote, out: &mut impl Write) -> Result<(), Failure> {
    let mut json = false;
    let mut prefix = String::new();
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            flag if flag.starts_with("--") => {
                return Err(Failure::Usage(format!("unknown option `{flag}`")));
            }
            text if prefix.is_empty() => text.clone_into(&mut prefix),
            extra => return Err(Failure::Usage(format!("unexpected argument `{extra}`"))),
        }
    }
    let catalog = remote.call(method::OPS, Value::Null)?;
    let rows: Vec<&Value> = callable(&catalog)
        .filter(|row| {
            row["id"].as_str().is_some_and(|id| id.starts_with(&prefix))
                || row["name"].as_str().is_some_and(|n| n.starts_with(&prefix))
        })
        .collect();
    let text = if json {
        serde_json::to_string_pretty(&rows).unwrap_or_default()
    } else {
        rows.iter()
            .map(|row| {
                format!(
                    "{:<26} {:<7} {}\n{:<34} {}",
                    row["id"].as_str().unwrap_or(""),
                    row["category"].as_str().unwrap_or(""),
                    row["signature"].as_str().unwrap_or(""),
                    "",
                    row["doc"].as_str().unwrap_or(""),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let _ = writeln!(out, "{text}");
    Ok(())
}

/// `gradiance op <id|name> …`.
fn invoke(args: &[String], remote: &mut impl Remote, out: &mut impl Write) -> Result<(), Failure> {
    let Some((op, rest)) = args.split_first() else {
        return Err(Failure::Usage("name an operation".into()));
    };
    let catalog = remote.call(method::OPS, Value::Null)?;
    let row = find(&catalog, op).ok_or_else(|| {
        Failure::Usage(format!(
            "no operation `{op}` (`gradiance ops` lists them; VM built-ins run only inside a script)"
        ))
    })?;
    let params = params_from_args(row, rest).map_err(Failure::Usage)?;
    let id = row["id"].clone();
    let value = remote.call(method::OP, serde_json::json!({"id": id, "params": params}))?;
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_default()
    );
    Ok(())
}

/// The named params object `args` give for the operation `row` describes:
/// positional args fill the declared fields in order, `--field value` (or
/// `--field=value`) sets one by name (`-` and `_` are the same), and
/// `--params JSON` gives a base object the others override.
pub fn params_from_args(row: &Value, args: &[String]) -> Result<Value, String> {
    let fields: Vec<&str> = row["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let schema = &row["schema"]["properties"];
    let mut params = Map::new();
    let mut positional = 0;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let Some(flag) = arg.strip_prefix("--") else {
            let field = fields.get(positional).ok_or_else(|| {
                format!(
                    "too many arguments: `{}` takes {} ({})",
                    row["id"].as_str().unwrap_or(""),
                    fields.len(),
                    fields.join(" ")
                )
            })?;
            positional += 1;
            params.insert((*field).to_owned(), coerce(arg, &schema[*field])?);
            continue;
        };
        let (name, inline) = match flag.split_once('=') {
            Some((name, value)) => (name, Some(value.to_owned())),
            None => (flag, None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| iter.next().cloned())
                .ok_or_else(|| format!("`--{name}` needs a value"))
        };
        if name == "params" {
            let base: Value = serde_json::from_str(&value()?)
                .map_err(|err| format!("`--params` is not JSON: {err}"))?;
            let Value::Object(base) = base else {
                return Err("`--params` must be a JSON object".into());
            };
            for (key, val) in base {
                params.entry(key).or_insert(val);
            }
            continue;
        }
        let field = fields
            .iter()
            .find(|f| **f == name || f.replace('_', "-") == name)
            .ok_or_else(|| {
                format!(
                    "`{}` has no field `{name}`; its fields: {}",
                    row["id"].as_str().unwrap_or(""),
                    if fields.is_empty() {
                        "none".to_owned()
                    } else {
                        fields.join(", ")
                    }
                )
            })?;
        params.insert((*field).to_owned(), coerce(&value()?, &schema[*field])?);
    }
    Ok(Value::Object(params))
}

/// One argument's text as the JSON value its field's `schema` asks for.
pub fn coerce(raw: &str, schema: &Value) -> Result<Value, String> {
    let kind = schema["type"].as_str();
    if kind == Some("string") {
        return Ok(Value::String(raw.to_owned()));
    }
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return Ok(value);
    }
    if raw.contains(',') {
        let items = &schema["items"];
        return raw
            .split(',')
            .map(|item| coerce(item.trim(), items))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    match kind {
        None => Ok(Value::String(raw.to_owned())),
        Some(kind) => Err(format!("`{raw}` is not a{} {kind}", article(kind))),
    }
}

fn article(kind: &str) -> &'static str {
    if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "n"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spawn_box() -> Value {
        json!({
            "id": "body.spawn_box", "name": "spawn-box",
            "fields": ["x", "y", "w", "h"],
            "schema": {"properties": {
                "x": {"type": "number"}, "y": {"type": "number"},
                "w": {"type": "number"}, "h": {"type": "number"}}}
        })
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn positional_and_named_args_give_the_same_object() {
        let row = spawn_box();
        let a = params_from_args(&row, &strings(&["0", "1", "1.6", "1.2"]));
        let b = params_from_args(
            &row,
            &strings(&["--x", "0", "--y=1", "--w", "1.6", "--h", "1.2"]),
        );
        assert_eq!(a, Ok(json!({"x": 0, "y": 1, "w": 1.6, "h": 1.2})));
        assert_eq!(a, b);
    }

    #[test]
    fn bad_args_name_the_fields() {
        let row = spawn_box();
        let err = params_from_args(&row, &strings(&["--z", "1"])).err();
        assert!(err.is_some_and(|e| e.contains("x, y, w, h")));
        let err = params_from_args(&row, &strings(&["1", "2", "3", "4", "5"])).err();
        assert!(err.is_some_and(|e| e.contains("too many")));
        let err = params_from_args(&row, &strings(&["--x"])).err();
        assert!(err.is_some_and(|e| e.contains("needs a value")));
        let err = params_from_args(&row, &strings(&["--x", "abc"])).err();
        assert_eq!(err.as_deref(), Some("`abc` is not a number"));
    }

    #[test]
    fn params_flag_is_a_base_the_flags_override() {
        let row = spawn_box();
        let p = params_from_args(
            &row,
            &strings(&["--x", "5", "--params", r#"{"x": 1, "y": 2}"#]),
        );
        assert_eq!(p, Ok(json!({"x": 5, "y": 2})));
    }

    #[test]
    fn coercion_follows_the_schema_type() {
        let string = json!({"type": "string"});
        let any = json!({"description": "Target"});
        let points = json!({"type": "array", "items": {"type": "number"}});
        assert_eq!(coerce("12", &string), Ok(json!("12")));
        assert_eq!(coerce("12", &any), Ok(json!(12)));
        assert_eq!(coerce("selection", &any), Ok(json!("selection")));
        assert_eq!(coerce(r#"{"at":[1,2]}"#, &any), Ok(json!({"at": [1, 2]})));
        assert_eq!(coerce("1, 2.5", &points), Ok(json!([1, 2.5])));
        assert_eq!(coerce("true", &json!({"type": "boolean"})), Ok(json!(true)));
        assert!(coerce("yes", &json!({"type": "integer"})).is_err());
    }

    #[test]
    fn dashes_name_underscored_fields() {
        let row = json!({"id": "a.b", "fields": ["max_iters"],
            "schema": {"properties": {"max_iters": {"type": "integer"}}}});
        assert_eq!(
            params_from_args(&row, &strings(&["--max-iters", "3"])),
            Ok(json!({"max_iters": 3}))
        );
    }

    struct Fake(Vec<(String, Value)>);

    impl Remote for Fake {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
            self.0.push((method.to_owned(), params.clone()));
            match method {
                method::OPS => Ok(json!([spawn_box(), {"id": null, "name": "tool-x"}])),
                method::OP if params["params"]["x"] == 99 => Err(CallError::Rpc {
                    code: -32602,
                    message: "body.spawn_box: refused".into(),
                }),
                _ => Ok(json!("6f1c")),
            }
        }
    }

    fn run_with(command: &str, args: &[&str]) -> (i32, String, String, Fake) {
        let mut fake = Fake(Vec::new());
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(command, &strings(args), &mut fake, &mut out, &mut err);
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
            fake,
        )
    }

    #[test]
    fn op_calls_the_id_form_and_prints_the_value() {
        let (code, out, _, fake) = run_with("op", &["spawn-box", "0", "1", "1", "1"]);
        assert_eq!((code, out.trim()), (0, "\"6f1c\""));
        assert_eq!(
            fake.0.last(),
            Some(&(
                method::OP.to_owned(),
                json!({"id": "body.spawn_box", "params": {"x": 0, "y": 1, "w": 1, "h": 1}})
            ))
        );
    }

    #[test]
    fn exit_codes_separate_usage_from_refusal() {
        assert_eq!(run_with("op", &["tool-x"]).0, EXIT_USAGE);
        assert_eq!(run_with("op", &[]).0, EXIT_USAGE);
        let (code, _, err, _) = run_with("op", &["body.spawn_box", "--x", "99"]);
        assert_eq!(code, EXIT_FAILED);
        assert!(err.contains("refused"));
    }

    #[test]
    fn ops_lists_only_callable_rows() {
        let (code, out, _, _) = run_with("ops", &[]);
        assert_eq!(code, 0);
        assert!(out.contains("body.spawn_box"));
        assert!(!out.contains("tool-x"));
        let (_, out, _, _) = run_with("ops", &["panel.", "--json"]);
        assert_eq!(out.trim(), "[]");
    }
}
