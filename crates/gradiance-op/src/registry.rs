//! The [`Operation`] trait, its metadata and the [`OpRegistry`].

use crate::context::{OpContext, OpContextFillers, OpIdSource};
use crate::output::{OpOutput, apply};
use crate::reflect_path::PathError;
use bevy::prelude::*;
use bevy::reflect::{TypeInfo, Typed};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::borrow::Cow;

/// Which sanctioned seam an operation writes through. The seam decides the
/// undo behaviour (`docs/ui-design.md` § 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Seam {
    /// Authored-world edit: lowers to an undoable intent.
    Edit,
    /// Settings write (the invariant-4 seam).
    Config,
    /// Read only: lowers to a value and no write.
    Query,
    /// Non-authored editor state.
    Editor,
}

/// The metadata of one operation, declared once and projected to every
/// surface (Lisp name, console docs, remote catalog, menu row).
#[derive(Debug, Clone, Copy)]
pub struct OpMeta {
    /// Dotted, namespaced id: `body.spawn_box`.
    pub id: &'static str,
    /// The Lisp verb: `spawn-box`.
    pub lisp: &'static str,
    /// One-line description.
    pub doc: &'static str,
    /// The seam it writes through.
    pub seam: Seam,
    /// Menu path (`"Edit/Undo"`), for an operation that has a menu row.
    pub menu: Option<&'static str>,
    /// Default key chord, as text. Metadata only (keys are out of scope).
    pub chord: Option<&'static str>,
    /// Label of the undo step the operation records, if it records one.
    pub undo: Option<&'static str>,
    /// Sample params as a JSON object. Documentation, and the input of the
    /// conformance tests.
    pub example: &'static str,
}

/// One operation: a typed parameter struct that knows how to lower itself.
///
/// The struct is **not** the intent. Intents hold what undo needs (old and new
/// values, full records); the params hold what a caller writes. `lower` reads
/// the old values from the context's mirrors and the new ids from
/// [`OpContext::new_id`].
pub trait Operation: DeserializeOwned + Typed + 'static {
    /// The operation's metadata.
    const META: OpMeta;

    /// Turns the params into the writes and the value. Reads only the context.
    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError>;
}

/// Why an invocation did not produce an output.
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    /// No operation has this id (or Lisp name).
    #[error("unknown operation `{0}`")]
    Unknown(String),
    /// The params did not decode.
    #[error("{op}: {why}")]
    Decode {
        /// The operation's id.
        op: &'static str,
        /// serde's message.
        why: String,
    },
    /// A positional call had the wrong number of arguments.
    #[error("{op}: expected {expected} arguments, got {got}")]
    Arity {
        /// The operation's id.
        op: &'static str,
        /// The number of declared fields.
        expected: usize,
        /// The number of arguments given.
        got: usize,
    },
    /// The context holds no mirror of the type the operation reads.
    #[error("no `{0}` mirror in the operation context")]
    MissingMirror(&'static str),
    /// A settings write did not resolve.
    #[error(transparent)]
    Setting(#[from] PathError),
    /// The operation refused its params.
    #[error("{0}")]
    Invalid(String),
}

/// One registered operation, type-erased.
#[derive(Debug, Clone)]
pub struct OpEntry {
    /// The declared metadata.
    pub meta: OpMeta,
    /// The params' field names in declared order (from reflection).
    pub fields: Vec<&'static str>,
    lower: fn(Value, &OpContext) -> Result<OpOutput, OpError>,
}

impl OpEntry {
    fn of<O: Operation>() -> Self {
        let fields = match O::type_info() {
            TypeInfo::Struct(info) => info.field_names().to_vec(),
            _ => Vec::new(),
        };
        Self {
            meta: O::META,
            fields,
            lower: |params, ctx| {
                let op: O = serde_json::from_value(params).map_err(|err| OpError::Decode {
                    op: O::META.id,
                    why: err.to_string(),
                })?;
                op.lower(ctx)
            },
        }
    }

    /// Decodes `params` (a named JSON object) and lowers the operation.
    pub fn lower(&self, params: Value, ctx: &OpContext) -> Result<OpOutput, OpError> {
        (self.lower)(params, ctx)
    }

    /// Maps positional arguments to the named object, by declared field
    /// order. The Lisp doorway and the remote `args` form share it.
    pub fn named(&self, args: Vec<Value>) -> Result<Value, OpError> {
        if args.len() != self.fields.len() {
            return Err(OpError::Arity {
                op: self.meta.id,
                expected: self.fields.len(),
                got: args.len(),
            });
        }
        let object: Map<String, Value> = self
            .fields
            .iter()
            .map(|name| (*name).to_owned())
            .zip(args)
            .collect();
        Ok(Value::Object(object))
    }

    /// The Lisp call form: `(spawn-box x y w h)`.
    pub fn signature(&self) -> String {
        let mut text = format!("({}", self.meta.lisp);
        for field in &self.fields {
            text.push(' ');
            text.push_str(field);
        }
        text.push(')');
        text
    }
}

/// Every registered operation, in registration order.
#[derive(Resource, Debug, Default, Clone)]
pub struct OpRegistry {
    entries: Vec<OpEntry>,
}

impl OpRegistry {
    /// Registers `O`. A second operation with the same id or Lisp name is
    /// refused and logged: two declarations of one name is the drift this
    /// registry removes.
    pub fn register<O: Operation>(&mut self) {
        let entry = OpEntry::of::<O>();
        if self
            .entries
            .iter()
            .any(|e| e.meta.id == entry.meta.id || e.meta.lisp == entry.meta.lisp)
        {
            error!(
                id = entry.meta.id,
                "operation registered twice; kept the first"
            );
            return;
        }
        self.entries.push(entry);
    }

    /// The operation with this id.
    pub fn get(&self, id: &str) -> Option<&OpEntry> {
        self.entries.iter().find(|e| e.meta.id == id)
    }

    /// The operation with this Lisp name.
    pub fn by_lisp(&self, name: &str) -> Option<&OpEntry> {
        self.entries.iter().find(|e| e.meta.lisp == name)
    }

    /// Every entry, in registration order.
    pub fn entries(&self) -> &[OpEntry] {
        &self.entries
    }
}

/// Lowers and applies one operation by id: the path of every doorway that
/// invokes once per call (remote, menus). Builds a fresh context, so the call
/// reads committed state.
pub fn invoke(world: &mut World, id: &str, params: Value) -> Result<Option<Value>, OpError> {
    let entry = world
        .get_resource::<OpRegistry>()
        .and_then(|registry| registry.get(id).cloned())
        .ok_or_else(|| OpError::Unknown(id.to_owned()))?;
    let ctx = OpContext::build(world);
    let output = entry.lower(params, &ctx)?;
    Ok(apply(world, entry.meta.id, output))
}

/// A request to invoke one operation: what a menu row, a button or a key
/// handler writes in place of an intent. Drained in [`OpInvokeSet`].
#[derive(Message, Debug, Clone)]
pub struct InvokeOp {
    /// The operation's id.
    pub id: Cow<'static, str>,
    /// Named params (a JSON object).
    pub params: Value,
}

impl InvokeOp {
    /// An invocation of an operation that takes no params.
    pub fn new(id: &'static str) -> Self {
        Self {
            id: Cow::Borrowed(id),
            params: Value::Object(Map::new()),
        }
    }
}

/// The set that drains [`InvokeOp`]. The command layer orders its dispatcher
/// after it, so an invoked edit becomes a command in the same frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpInvokeSet;

/// Exclusive: lowers and applies each pending [`InvokeOp`]. A failed
/// invocation is logged and dropped.
fn run_invocations(world: &mut World) {
    for request in gradiance_core::messages::drain::<InvokeOp>(world) {
        if let Err(err) = invoke(world, &request.id, request.params) {
            warn!("{err}");
        }
    }
}

/// Installs the registry, the context builder's table, the id source and the
/// [`InvokeOp`] doorway. Each layer then registers its own operations.
#[derive(Default)]
pub struct OpPlugin;

impl Plugin for OpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpRegistry>();
        app.init_resource::<OpContextFillers>();
        app.init_resource::<OpIdSource>();
        app.add_message::<InvokeOp>();
        app.add_systems(Update, run_invocations.in_set(OpInvokeSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Message, Debug, Clone, PartialEq)]
    struct Moved(f32, f32);

    /// Move a thing.
    #[derive(Deserialize, Reflect)]
    #[serde(deny_unknown_fields)]
    struct MoveThing {
        x: f32,
        y: f32,
    }

    impl Operation for MoveThing {
        const META: OpMeta = OpMeta {
            id: "thing.move",
            lisp: "move-thing",
            doc: "Move a thing.",
            seam: Seam::Edit,
            menu: None,
            chord: None,
            undo: Some("Move"),
            example: r#"{"x": 1, "y": 2}"#,
        };

        fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
            Ok(OpOutput::none().message(Moved(self.x, self.y)))
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(OpPlugin);
        app.add_message::<Moved>();
        app.world_mut()
            .resource_mut::<OpRegistry>()
            .register::<MoveThing>();
        app
    }

    #[test]
    fn fields_and_signature_come_from_the_struct() {
        let entry = OpEntry::of::<MoveThing>();
        assert_eq!(entry.fields, ["x", "y"]);
        assert_eq!(entry.signature(), "(move-thing x y)");
    }

    #[test]
    fn positional_and_named_calls_lower_to_the_same_output() {
        let entry = OpEntry::of::<MoveThing>();
        let ctx = OpContext::new(OpIdSource::Random);
        let named = entry.lower(json!({"x": 1, "y": 2.5}), &ctx);
        let positional = entry
            .named(vec![json!(1), json!(2.5)])
            .and_then(|params| entry.lower(params, &ctx));
        let describe = |r: Result<OpOutput, OpError>| r.map(|o| o.describe()).ok();
        assert_eq!(describe(named), Some(vec!["Moved(1.0, 2.5)".to_owned()]));
        assert_eq!(
            describe(positional),
            Some(vec!["Moved(1.0, 2.5)".to_owned()])
        );
    }

    #[test]
    fn bad_calls_are_errors_that_name_the_operation() {
        let entry = OpEntry::of::<MoveThing>();
        let ctx = OpContext::new(OpIdSource::Random);
        let text = |r: Result<OpOutput, OpError>| r.err().map(|e| e.to_string());
        let wrong_type = text(entry.lower(json!({"x": "left", "y": 0}), &ctx));
        assert!(wrong_type.is_some_and(|t| t.starts_with("thing.move: ")));
        let unknown_field = text(entry.lower(json!({"x": 0, "y": 0, "z": 0}), &ctx));
        assert!(unknown_field.is_some_and(|t| t.contains("unknown field")));
        let arity = entry.named(vec![json!(1)]).err().map(|e| e.to_string());
        assert_eq!(
            arity.as_deref(),
            Some("thing.move: expected 2 arguments, got 1")
        );
    }

    #[test]
    fn an_invoke_message_reaches_the_queue_in_the_same_update() {
        let mut app = app();
        app.world_mut().write_message(InvokeOp {
            id: Cow::Borrowed("thing.move"),
            params: json!({"x": 3, "y": 4}),
        });
        app.update();
        assert_eq!(app.world().resource::<Messages<Moved>>().len(), 1);
    }

    #[test]
    fn a_duplicate_registration_is_refused() {
        let mut registry = OpRegistry::default();
        registry.register::<MoveThing>();
        registry.register::<MoveThing>();
        assert_eq!(registry.entries().len(), 1);
    }

    #[test]
    fn an_unknown_id_is_an_error() {
        let mut app = app();
        let err = invoke(app.world_mut(), "thing.fly", json!({})).err();
        assert!(matches!(err, Some(OpError::Unknown(_))));
    }
}
