//! Reflect-path reads and writes of scalars: `"gravity.y"` on any
//! `#[derive(Reflect)]` value, with no per-field code.
//!
//! Pure reflection glue (no ECS, no VM). A config operation uses it to write
//! one setting ([`OpOutput::setting`](crate::OpOutput::setting)); the script
//! layer re-exports it for its `…-get` verbs.

use bevy::reflect::{PartialReflect, Reflect, ReflectMut, ReflectPath, ReflectRef};

/// Why a reflect-path write could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The reflect path did not resolve to a field.
    #[error("reflect path `{0}` did not resolve")]
    Path(String),
    /// The leaf at the path is not a supported scalar type.
    #[error("unsupported scalar type at reflect path `{0}`")]
    UnsupportedLeaf(String),
}

/// Unwraps a dimensional newtype — a single-field tuple struct like
/// `Mass(f32)`, the shape every `gradiance-units` quantity takes — to its
/// inner value, recursively (nested newtypes), so the read/write paths treat
/// a typed quantity as the scalar it wraps. Non-newtypes (named structs,
/// multi-/zero-field tuple structs, and `#[reflect(opaque)]` handles like
/// `StableId`/`ShapeDef`) are returned unchanged, so opaque handles still
/// degrade to `Void` by design.
pub fn unwrap_newtype(value: &dyn PartialReflect) -> &dyn PartialReflect {
    if let ReflectRef::TupleStruct(ts) = value.reflect_ref()
        && ts.field_len() == 1
        && let Some(inner) = ts.field(0)
    {
        return unwrap_newtype(inner);
    }
    value
}

/// One `.`-separated segment of a script path, as a reflect-path access
/// (`y` -> `.y`, `0` -> `.0`, `lights[1]` -> `.lights[1]`).
fn segments(path: &str) -> impl Iterator<Item = String> + '_ {
    path.split('.')
        .filter(|s| !s.is_empty())
        .map(|s| format!(".{s}"))
}

/// Resolves a script path, stepping *through* a dimensional newtype wherever
/// the next segment names a field of the value it wraps — so `"gravity.y"`
/// reaches the `y` of `gravity: Acceleration2(Vec2)` exactly as it did when
/// the field was a bare `Vec2`. A segment that resolves on the newtype itself
/// (`"gravity.0"`) still does.
fn resolve<'r>(root: &'r dyn PartialReflect, path: &str) -> Option<&'r dyn PartialReflect> {
    let mut current = root;
    for segment in segments(path) {
        current = match segment.as_str().reflect_element(current) {
            Ok(next) => next,
            Err(_) => segment
                .as_str()
                .reflect_element(unwrap_newtype(current))
                .ok()?,
        };
    }
    Some(current)
}

/// [`resolve`], mutably.
fn resolve_mut<'r>(
    root: &'r mut dyn PartialReflect,
    path: &str,
) -> Option<&'r mut dyn PartialReflect> {
    let mut current = root;
    for segment in segments(path) {
        while segment.as_str().reflect_element(&*current).is_err() && is_newtype(&*current) {
            current = newtype_inner_mut(current)?;
        }
        current = segment.as_str().reflect_element_mut(current).ok()?;
    }
    Some(current)
}

/// Whether `value` is a single-field tuple struct (a dimensional newtype).
fn is_newtype(value: &dyn PartialReflect) -> bool {
    matches!(value.reflect_ref(), ReflectRef::TupleStruct(ts) if ts.field_len() == 1)
}

/// The wrapped field of a single-field tuple struct.
fn newtype_inner_mut(value: &mut dyn PartialReflect) -> Option<&mut dyn PartialReflect> {
    match value.reflect_mut() {
        ReflectMut::TupleStruct(ts) if ts.field_len() == 1 => ts.field_mut(0),
        _ => None,
    }
}

/// Read the scalar at a reflect path as `f64`. Generic over the concrete root
/// so the `GetPath` blanket impl applies (`&dyn PartialReflect` does not
/// satisfy it). A flag reads as 1 or 0. Returns `None` if the path is
/// missing or the leaf is not a numeric scalar or a flag.
pub fn read_path<T: Reflect>(root: &T, path: &str) -> Option<f64> {
    let leaf = unwrap_newtype(resolve(root.as_partial_reflect(), path)?);
    let any = leaf.try_as_reflect()?.as_any();
    any.downcast_ref::<f32>()
        .map(|v| f64::from(*v))
        .or_else(|| any.downcast_ref::<f64>().copied())
        .or_else(|| any.downcast_ref::<u32>().map(|v| f64::from(*v)))
        .or_else(|| any.downcast_ref::<i32>().map(|v| f64::from(*v)))
        .or_else(|| any.downcast_ref::<bool>().map(|v| f64::from(u8::from(*v))))
}

/// Write an `f64` onto the scalar at a reflect path, coercing to the leaf's
/// concrete type (`f32`/`f64`/`u32`/`i32`/`bool`) and descending through a
/// dimensional newtype wrapper (`Mass(f32)`) to its inner scalar. Never names
/// a field, so it works for any reflected type.
pub fn write_path<T: Reflect>(root: &mut T, path: &str, val: f64) -> Result<(), PathError> {
    let leaf = resolve_mut(root.as_partial_reflect_mut(), path)
        .ok_or_else(|| PathError::Path(path.to_string()))?;
    apply_scalar(leaf, val).map_err(|()| PathError::UnsupportedLeaf(path.to_string()))
}

/// Applies `val` to a scalar leaf, or descends one level into a single-field
/// tuple-struct newtype and applies to its inner field (recursively).
fn apply_scalar(leaf: &mut dyn PartialReflect, val: f64) -> Result<(), ()> {
    if try_apply_scalar(leaf, val) {
        return Ok(());
    }
    if let ReflectMut::TupleStruct(ts) = leaf.reflect_mut()
        && ts.field_len() == 1
        && let Some(inner) = ts.field_mut(0)
    {
        return apply_scalar(inner, val);
    }
    Err(())
}

/// Tries each supported scalar type in turn; `true` if one applied.
fn try_apply_scalar(leaf: &mut dyn PartialReflect, val: f64) -> bool {
    leaf.try_apply(&(val as f32) as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&val as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&(val as u32) as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&(val as i32) as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&(val != 0.0) as &dyn PartialReflect).is_ok()
}
