//! The external control surface: Bevy's remote protocol (BRP, JSON-RPC 2.0
//! over HTTP) bound to the **operation registry**, so a process outside the
//! app drives Gradiance through the same verbs a script does.
//!
//! Three methods (`docs/scripting.md` § Remote control):
//!
//! - [`method::OPS`] — the catalog: every op's name, signature, doc,
//!   governance category and arity.
//! - [`method::OP`] — invoke one **registry operation** (`gradiance-op`), in
//!   one of two forms:
//!   - `{"id": "body.spawn_box", "params": {"x": 0, "y": 2, "w": 1, "h": 1}}`
//!     — named params, decoded by the registry's one serde decoder and
//!     applied through the seams the operation names. Its edits reach the
//!     dispatcher in the next `Update`, like a console line's.
//!   - `{"name": "spawn-box", "args": [0, 2, 1, 1]}` — the same operation by
//!     its Lisp name with positional args. The args map to the named params
//!     by declared field order ([`OpEntry::named`](gradiance_op::OpEntry::named),
//!     the mapping the Lisp doorway uses) and the call goes through the
//!     registry exactly as the `id` form does.
//!
//!   The surface is registry-only: no call builds Scheme text or runs the
//!   VM. A **VM built-in** (`crate::registry::BUILTINS`: the
//!   invocation-context reads and `ops`/`describe`) exists only inside a
//!   script run, so naming one is refused with an invalid-params error that
//!   says so (brief U-3b); `gradiance.ops` is the remote form of `(ops)`.
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

use crate::bridge::OperationRegistry;
use crate::registry::{BUILTINS, CatalogEntry, OpCategory};
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

/// One call in either form: a registry operation by `id`, or by Lisp `name`
/// with positional `args`. A VM built-in's name is refused.
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
    let Some(entry) = entry else {
        return Err(invalid_params(unknown(&name)));
    };
    let params = entry
        .named(args)
        .map_err(|err| invalid_params(format!("{name}: {err}")))?;
    invoke_entry(world, entry.meta.id, entry.meta.seam, params, only)
}

/// Why `name` is not callable: a VM built-in exists only inside a script
/// run; anything else is unknown.
fn unknown(name: &str) -> String {
    if BUILTINS.iter().any(|builtin| builtin.name == name) {
        format!(
            "`{name}` is a VM built-in: it exists only inside a script run, not on the remote surface"
        )
    } else {
        format!("unknown operation `{name}`")
    }
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

fn invalid_params(message: impl Into<String>) -> BrpError {
    BrpError {
        code: error_codes::INVALID_PARAMS,
        message: message.into(),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_builtin_is_refused_as_living_in_a_script_run() {
        assert!(unknown("tool-x").contains("only inside a script run"));
        assert_eq!(unknown("no-such-op"), "unknown operation `no-such-op`");
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
