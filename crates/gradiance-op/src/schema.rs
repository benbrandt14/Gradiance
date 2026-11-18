//! A JSON Schema for an operation's params, built from reflection.
//!
//! Reflection describes; it does not decode (`docs/unified-surface-plan.md`
//! § 13.3). The schema tells a doorway (the CLI's flag parser, an MCP client)
//! what shape each field takes. The registry's serde decoder stays the one
//! authority on what is accepted: a type the schema cannot name (an untagged
//! enum such as [`Target`](crate::Target), a math vector) is described by its
//! type path, and the operation's `example` shows its JSON form.

use bevy::reflect::TypeInfo;
use bevy::reflect::enums::VariantInfo;
use serde_json::{Map, Value, json};

/// The params object schema of a reflected struct: one property per field,
/// in declared order. A field is required unless its type is `Option<_>`
/// (no operation params struct uses `#[serde(default)]`; the conformance test
/// pins that every example decodes). Unknown fields are refused, as the
/// decoder refuses them (`deny_unknown_fields`).
pub(crate) fn params_schema(info: &TypeInfo) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    if let TypeInfo::Struct(info) = info {
        for field in info.iter() {
            let (schema, optional) = match field.type_info() {
                Some(info) => match option_inner(info) {
                    Some(inner) => (inner, true),
                    None => (field_schema(info), false),
                },
                None => (described(field.type_path()), false),
            };
            properties.insert(field.name().to_owned(), schema);
            if !optional {
                required.push(Value::from(field.name()));
            }
        }
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// The schema of one value type.
fn field_schema(info: &TypeInfo) -> Value {
    if let Some(kind) = primitive(info.type_path()) {
        return json!({ "type": kind });
    }
    match info {
        TypeInfo::List(list) => array(list.item_info(), list.item_ty().path(), None),
        TypeInfo::Array(array_info) => array(
            array_info.item_info(),
            array_info.item_ty().path(),
            Some(array_info.capacity()),
        ),
        _ => described(info.type_path()),
    }
}

/// An array of `item`; `len` fixes its length.
fn array(item: Option<&TypeInfo>, item_path: &str, len: Option<usize>) -> Value {
    let mut schema = Map::new();
    schema.insert("type".into(), "array".into());
    schema.insert(
        "items".into(),
        item.map_or_else(|| described(item_path), field_schema),
    );
    if let Some(len) = len {
        schema.insert("minItems".into(), len.into());
        schema.insert("maxItems".into(), len.into());
    }
    Value::Object(schema)
}

/// A type the schema cannot name: its short type path, as documentation.
fn described(type_path: &str) -> Value {
    json!({ "description": short_path(type_path) })
}

/// The schema of `inner` if `info` is `Option<inner>` (an empty schema when
/// the inner type has no static info); `None` for any other type.
fn option_inner(info: &TypeInfo) -> Option<Value> {
    let TypeInfo::Enum(info) = info else {
        return None;
    };
    if !info.type_path().starts_with("core::option::Option<") {
        return None;
    }
    let inner = match info.variant("Some") {
        Some(VariantInfo::Tuple(some)) => some
            .field_at(0)
            .and_then(bevy::reflect::UnnamedField::type_info),
        _ => None,
    };
    Some(inner.map_or_else(|| json!({}), field_schema))
}

/// The JSON Schema type of a primitive type path.
fn primitive(type_path: &str) -> Option<&'static str> {
    Some(match type_path {
        "f32" | "f64" => "number",
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => "integer",
        "bool" => "boolean",
        "alloc::string::String" | "&str" | "char" | "alloc::borrow::Cow<str>" => "string",
        _ => return None,
    })
}

/// `gradiance_op::target::Target` → `Target`; generic arguments are
/// shortened the same way.
fn short_path(type_path: &str) -> String {
    let mut out = String::with_capacity(type_path.len());
    let mut segment = String::new();
    for c in type_path.chars() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            segment.push(c);
        } else {
            out.push_str(segment.rsplit("::").next().unwrap_or(&segment));
            segment.clear();
            out.push(c);
        }
    }
    out.push_str(segment.rsplit("::").next().unwrap_or(&segment));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Target;
    use bevy::reflect::{Reflect, Typed};

    #[derive(Reflect)]
    struct Params {
        x: f32,
        count: u32,
        on: bool,
        name: String,
        maybe: Option<f32>,
        list: Vec<i64>,
        point: [f32; 2],
        target: Target,
    }

    #[test]
    fn each_field_gets_its_json_type() {
        let schema = params_schema(Params::type_info());
        let p = &schema["properties"];
        assert_eq!(p["x"], json!({"type": "number"}));
        assert_eq!(p["count"], json!({"type": "integer"}));
        assert_eq!(p["on"], json!({"type": "boolean"}));
        assert_eq!(p["name"], json!({"type": "string"}));
        assert_eq!(p["maybe"], json!({"type": "number"}));
        assert_eq!(
            p["list"],
            json!({"type": "array", "items": {"type": "integer"}})
        );
        assert_eq!(
            p["point"],
            json!({"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2})
        );
        assert_eq!(p["target"], json!({"description": "Target"}));
    }

    #[test]
    fn only_option_fields_are_optional_and_order_is_declared() {
        let schema = params_schema(Params::type_info());
        assert_eq!(
            schema["required"],
            json!(["x", "count", "on", "name", "list", "point", "target"])
        );
        let keys = schema["properties"]
            .as_object()
            .map_or(0, serde_json::Map::len);
        assert_eq!(keys, 8);
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn short_paths_drop_modules_in_generics_too() {
        assert_eq!(short_path("a::b::Thing"), "Thing");
        assert_eq!(
            short_path("alloc::vec::Vec<gradiance_core::ids::StableId>"),
            "Vec<StableId>"
        );
    }
}
