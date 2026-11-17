//! The external control surface: Bevy's remote protocol (BRP, JSON-RPC 2.0
//! over HTTP) bound to the **operation registry**, so a process outside the
//! app drives Gradiance through the same verbs a script does.
//!
//! Three methods (`docs/scripting.md` § Remote control):
//!
//! - [`method::OPS`] — the catalog: every op's name, signature, doc,
//!   governance category and arity.
//! - [`method::OP`] — invoke one op, in one of two forms:
//!   - `{"id": "body.spawn_box", "params": {"x": 0, "y": 2, "w": 1, "h": 1}}`
//!     — a **registry operation** (`gradiance-op`) with named params, decoded
//!     by the registry's one serde decoder and applied through the seams it
//!     names. No Scheme text is built. Its edits reach the dispatcher in the
//!     next `Update`, like a console line's.
//!   - `{"name": "spawn-box", "args": [0, 2, 1, 1]}` — the same operation by
//!     its Lisp name with positional args. The args map to the named params
//!     by declared field order ([`OpEntry::named`](gradiance_op::OpEntry::named),
//!     the mapping the Lisp doorway uses) and the call goes through the
//!     registry exactly as the `id` form does. Only a **VM built-in**
//!     (`crate::registry::BUILTINS`: the panel verbs until U-3, the
//!     invocation-context reads, `ops`/`describe`) is still lowered to **one
//!     script run**: the args rendered as Scheme literals, submitted to
//!     [`ScriptInputs`] and run through [`run_scripts`] like a console line.
//! - [`method::QUERY`] — the same, restricted to `query` ops: a client
//!   holding only this method cannot change anything.
//!
//! **Security posture.** Off unless the app is started with `--remote`;
//! bound to `127.0.0.1` only; no authentication (out of scope — localhost is
//! the boundary). Bevy's built-in mutation methods (`world.spawn_entity`,
//! `world.insert_components`, `world.mutate_*`, `world.remove_*`,
//! `world.despawn_entity`, `world.reparent_entities`, `world.insert_resources`,
//! `world.trigger_event`, `world.write_message`) are **removed**: writes stay
//! seam-mediated. [`KEPT_BUILTINS`] lists the read-only ones that stay.

use crate::bridge::{OperationRegistry, ScriptInputs, run_scripts};
use crate::registry::{CatalogEntry, OpCategory};
use bevy::prelude::*;
use bevy::remote::builtin_methods::{
    BRP_GET_COMPONENTS_METHOD, BRP_GET_RESOURCE_METHOD, BRP_LIST_COMPONENTS_METHOD,
    BRP_LIST_RESOURCES_METHOD, BRP_QUERY_METHOD, BRP_REGISTRY_SCHEMA_METHOD, RPC_DISCOVER_METHOD,
};
use bevy::remote::http::RemoteHttpPlugin;
use bevy::remote::{BrpError, BrpResult, RemoteMethods, RemotePlugin, error_codes};
use gradiance_op::OpRegistry;
use serde_json::Map;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::net::Ipv4Addr;

/// The method names this surface adds.
pub mod method {
    /// List the operation catalog.
    pub const OPS: &str = "gradiance.ops";
    /// Invoke one operation by name with positional args.
    pub const OP: &str = "gradiance.op";
    /// Invoke one `query` operation (refuses every other category).
    pub const QUERY: &str = "gradiance.query";
}

/// Bevy's built-in methods that stay: discovery and reflection **reads**.
/// None of them changes the world; every other built-in (all the
/// `world.*` writes, the `+watch` streams and the `schedule.*` debug views)
/// is removed from the method table.
pub const KEPT_BUILTINS: &[&str] = &[
    RPC_DISCOVER_METHOD,
    BRP_GET_COMPONENTS_METHOD,
    BRP_QUERY_METHOD,
    BRP_LIST_COMPONENTS_METHOD,
    BRP_GET_RESOURCE_METHOD,
    BRP_LIST_RESOURCES_METHOD,
    BRP_REGISTRY_SCHEMA_METHOD,
];

/// The JSON-RPC error code of a run that failed (evaluation error, panic,
/// budget): in the spec's implementation-defined server-error range. The
/// error's `message` is the exact text the console log shows.
pub const SCRIPT_ERROR: i16 = -32000;

/// The port the HTTP transport listens on unless `--remote <port>` says
/// otherwise (Bevy's default BRP port).
pub const DEFAULT_PORT: u16 = bevy::remote::http::DEFAULT_PORT;

/// Turns the remote control surface on. Not part of the default plugin
/// group: the binary adds it only for `--remote`.
#[derive(Debug, Clone, Copy)]
pub struct RemoteControlPlugin {
    /// The localhost port to serve HTTP on; `None` registers the methods
    /// without a transport (in-process tests call them directly).
    pub port: Option<u16>,
}

impl Default for RemoteControlPlugin {
    fn default() -> Self {
        Self {
            port: Some(DEFAULT_PORT),
        }
    }
}

impl Plugin for RemoteControlPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            RemotePlugin::default()
                .with_method_main(method::OPS, list_ops)
                .with_method_main(method::OP, invoke_op)
                .with_method_main(method::QUERY, invoke_query),
        );
        if let Some(port) = self.port {
            app.add_plugins(
                RemoteHttpPlugin::default()
                    .with_address(Ipv4Addr::LOCALHOST)
                    .with_port(port),
            );
        }
    }

    fn finish(&self, app: &mut App) {
        // `RemotePlugin` has no empty constructor: its default registers every
        // built-in. Keep only ours and the read-only built-ins.
        let world = app.world_mut();
        if let Some(mut methods) = world.get_resource_mut::<RemoteMethods>() {
            *methods = curated(&methods);
        }
        // With a renderer, `RemotePlugin` also serves the render world (all
        // built-ins, writes included, on Bevy's render port). Gradiance has
        // nothing to expose there: empty its table.
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp)
            && let Some(mut methods) = render.world_mut().get_resource_mut::<RemoteMethods>()
        {
            *methods = RemoteMethods::new();
        }
    }
}

/// The method table with only Gradiance's methods and [`KEPT_BUILTINS`].
fn curated(all: &RemoteMethods) -> RemoteMethods {
    let mut kept = RemoteMethods::new();
    for name in [method::OPS, method::OP, method::QUERY]
        .iter()
        .chain(KEPT_BUILTINS)
    {
        if let Some(id) = all.get(name) {
            kept.insert(*name, *id);
        }
    }
    kept
}

/// `gradiance.ops`: the catalog as a JSON array.
#[expect(
    clippy::unnecessary_wraps,
    reason = "a BRP method handler must return `BrpResult`"
)]
fn list_ops(
    In(_params): In<Option<Value>>,
    catalog: Res<OperationRegistry>,
    registry: Res<OpRegistry>,
) -> BrpResult {
    Ok(Value::Array(
        catalog
            .0
            .ops()
            .iter()
            .map(|op| op_json(op, &registry))
            .collect(),
    ))
}

/// One catalog row; a registry operation also carries its `id` and its
/// declared field names (the keys of the `params` form).
fn op_json(op: &CatalogEntry, registry: &OpRegistry) -> Value {
    let entry = registry.by_lisp(op.name);
    json!({
        "id": entry.map(|e| e.meta.id),
        "fields": entry.map(|e| e.fields.clone()),
        "name": op.name,
        "signature": op.signature,
        "doc": op.doc,
        "category": op.category.label(),
        "args": op.args,
    })
}

/// `gradiance.op`: any registered operation.
fn invoke_op(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    invoke(params, world, None)
}

/// `gradiance.query`: a `query` operation only.
fn invoke_query(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    invoke(params, world, Some(OpCategory::Query))
}

/// One call in either form. A registry operation (by `id`, or by Lisp `name`
/// with positional `args`) goes through the registry; a VM built-in runs as
/// one script run.
fn invoke(params: Option<Value>, world: &mut World, only: Option<OpCategory>) -> BrpResult {
    if let Some(Value::Object(fields)) = &params
        && fields.contains_key("id")
    {
        return invoke_registered(params, world, only);
    }
    let (name, args) = parse_params(params)?;
    let entry = world
        .get_resource::<OpRegistry>()
        .and_then(|registry| registry.by_lisp(&name).cloned());
    if let Some(entry) = entry {
        let params = entry
            .named(args)
            .map_err(|err| invalid_params(format!("{name}: {err}")))?;
        return invoke_entry(world, entry.meta.id, entry.meta.seam, params, only);
    }
    let spec = world
        .get_resource::<OperationRegistry>()
        .and_then(|registry| registry.0.find(&name).cloned())
        .ok_or_else(|| invalid_params(format!("unknown operation `{name}`")))?;
    check_category(&name, spec.category, only)?;
    let source = call_source(spec.name, &args)?;

    let reply = world
        .get_resource_mut::<ScriptInputs>()
        .map(|mut inputs| inputs.submit_with_reply(source.clone()))
        .ok_or_else(|| internal("the script plugin is not installed"))?;
    run_scripts(world);

    let outcome = reply
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .ok_or_else(|| internal("the script run left no outcome"))?;
    outcome.map_err(|message| BrpError {
        code: SCRIPT_ERROR,
        message,
        data: Some(json!({ "source": source })),
    })
}

/// Refuses a call whose category `only` excludes.
fn check_category(
    name: &str,
    category: OpCategory,
    only: Option<OpCategory>,
) -> Result<(), BrpError> {
    match only {
        Some(only) if category != only => Err(invalid_params(format!(
            "`{name}` is a {} operation; `{}` runs {} operations only",
            category.label(),
            method::QUERY,
            only.label(),
        ))),
        _ => Ok(()),
    }
}

/// Lowers one registry operation against a fresh context and applies it.
/// The reply is the operation's value (`null` for none). An error names the
/// operation once.
fn invoke_entry(
    world: &mut World,
    id: &'static str,
    seam: gradiance_op::Seam,
    params: Value,
    only: Option<OpCategory>,
) -> BrpResult {
    check_category(id, OpCategory::from(seam), only)?;
    gradiance_op::invoke(world, id, params)
        .map(|value| value.unwrap_or(Value::Null))
        .map_err(|err| invalid_params(format!("{id}: {err}")))
}

/// `{"id": <string>, "params": {…}}`: one registry operation by id.
fn invoke_registered(
    params: Option<Value>,
    world: &mut World,
    only: Option<OpCategory>,
) -> BrpResult {
    let Some(Value::Object(mut fields)) = params else {
        return Err(invalid_params("params must be an object"));
    };
    let Some(Value::String(id)) = fields.remove("id") else {
        return Err(invalid_params("`id` must be the operation's id, a string"));
    };
    let op_params = match fields.remove("params") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(object @ Value::Object(_)) => object,
        Some(_) => return Err(invalid_params("`params` must be an object")),
    };
    let (id, seam) = world
        .get_resource::<OpRegistry>()
        .and_then(|registry| registry.get(&id).map(|e| (e.meta.id, e.meta.seam)))
        .ok_or_else(|| invalid_params(format!("unknown operation `{id}`")))?;
    invoke_entry(world, id, seam, op_params, only)
}

/// Reads `{"name": <string>, "args": [<arg>…]}` (`args` optional).
fn parse_params(params: Option<Value>) -> Result<(String, Vec<Value>), BrpError> {
    let Some(Value::Object(mut fields)) = params else {
        return Err(invalid_params(
            "params must be an object: {\"name\": <op>, \"args\": [...]}",
        ));
    };
    let Some(Value::String(name)) = fields.remove("name") else {
        return Err(invalid_params(
            "`name` must be the operation's name, a string",
        ));
    };
    let args = match fields.remove("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(args)) => args,
        Some(_) => return Err(invalid_params("`args` must be an array")),
    };
    Ok((name, args))
}

/// `(name arg…)` with every argument rendered as a Scheme literal: the form
/// a VM built-in's remote call runs as. The conformance tests also use it to
/// drive the Lisp doorway from an operation's example.
pub fn call_source(name: &str, args: &[Value]) -> Result<String, BrpError> {
    let mut source = format!("({name}");
    for (i, arg) in args.iter().enumerate() {
        source.push(' ');
        literal(arg, &mut source).map_err(|why| invalid_params(format!("argument {i}: {why}")))?;
    }
    source.push(')');
    Ok(source)
}

/// Appends `value` as a Scheme literal: numbers as numbers, strings quoted
/// and escaped, booleans as `#t`/`#f`, arrays as `(list …)`. `null` and
/// objects have no literal form and are refused.
fn literal(value: &Value, out: &mut String) -> Result<(), &'static str> {
    match value {
        Value::Bool(b) => out.push_str(if *b { "#t" } else { "#f" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(f) = n.as_f64() {
                // `{:?}` keeps a decimal point (`1.0`), so the VM reads a float.
                let _ = write!(out, "{f:?}");
            } else {
                return Err("number out of range");
            }
        }
        Value::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::Array(items) => {
            out.push_str("(list");
            for item in items {
                out.push(' ');
                literal(item, out)?;
            }
            out.push(')');
        }
        Value::Null => return Err("null has no Scheme form"),
        Value::Object(_) => return Err("objects have no Scheme form"),
    }
    Ok(())
}

fn invalid_params(message: impl Into<String>) -> BrpError {
    BrpError {
        code: error_codes::INVALID_PARAMS,
        message: message.into(),
        data: None,
    }
}

fn internal(message: &str) -> BrpError {
    BrpError {
        code: error_codes::INTERNAL_ERROR,
        message: message.to_owned(),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_render_as_scheme_literals() {
        let args = [
            json!(0),
            json!(-2.5),
            json!(true),
            json!("a \"b\"\n"),
            json!([1, "x"]),
        ];
        assert_eq!(
            call_source("f", &args).ok().as_deref(),
            Some(r#"(f 0 -2.5 #t "a \"b\"\n" (list 1 "x"))"#)
        );
        assert_eq!(call_source("ops", &[]).ok().as_deref(), Some("(ops)"));
    }

    #[test]
    fn whole_floats_stay_floats() {
        assert_eq!(
            call_source("f", &[json!(1.0)]).ok().as_deref(),
            Some("(f 1.0)")
        );
    }

    #[test]
    fn null_and_objects_are_refused() {
        for bad in [json!(null), json!({"x": 1})] {
            let err = call_source("f", &[json!(1), bad]).err();
            assert_eq!(err.map(|e| e.code), Some(error_codes::INVALID_PARAMS));
        }
    }

    #[test]
    fn params_must_name_an_op() {
        assert!(parse_params(None).is_err());
        assert!(parse_params(Some(json!([1, 2]))).is_err());
        assert!(parse_params(Some(json!({"args": []}))).is_err());
        assert!(parse_params(Some(json!({"name": "f", "args": 3}))).is_err());
        let (name, args) =
            parse_params(Some(json!({"name": "body-count"}))).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!((name.as_str(), args.len()), ("body-count", 0));
    }
}
