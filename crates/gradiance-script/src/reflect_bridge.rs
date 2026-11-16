//! The `bevy_reflect` <-> steel value bridge (Tier-A authoring path).
//!
//! One generic, reflection-driven bridge makes every `#[derive(Reflect)]`
//! type scriptable with no per-field code: a script reads a value by
//! reflect-path, writes a scalar back onto whatever concrete type lives
//! there, and converts a whole value to steel data for the "reads are total"
//! path. Validated by Spike 1 (`docs/script-spike-findings.md`).
//!
//! Alongside [`bridge`](crate::bridge) this is one of the only two
//! places `steel` may be imported (enforced by `tests/boundaries.rs`). It is
//! pure w.r.t. the ECS — it takes plain reflected values; the World-facing
//! seam that dispatches through intents/settings resources builds on top of
//! it.

use bevy::reflect::{PartialReflect, Reflect, ReflectMut, ReflectPath, ReflectRef};
use steel::rvals::SteelVal;

/// Why a reflect-path write could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BridgeError {
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
fn unwrap_newtype(value: &dyn PartialReflect) -> &dyn PartialReflect {
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

/// A reflected leaf scalar -> a steel value, if it is a supported scalar
/// (`f32`, `f64`, `u32`, `i32`, `bool`) — unwrapping a dimensional newtype
/// wrapper first.
pub fn scalar_to_steel(leaf: &dyn PartialReflect) -> Option<SteelVal> {
    let any = unwrap_newtype(leaf).try_as_reflect()?.as_any();
    any.downcast_ref::<f32>()
        .map(|v| SteelVal::NumV(f64::from(*v)))
        .or_else(|| any.downcast_ref::<f64>().map(|v| SteelVal::NumV(*v)))
        .or_else(|| {
            any.downcast_ref::<u32>()
                .map(|v| SteelVal::IntV(isize::try_from(*v).unwrap_or(isize::MAX)))
        })
        .or_else(|| {
            any.downcast_ref::<i32>()
                .map(|v| SteelVal::IntV(isize::try_from(*v).unwrap_or(isize::MAX)))
        })
        .or_else(|| any.downcast_ref::<bool>().map(|v| SteelVal::BoolV(*v)))
}

/// Any reflected value -> steel data: structs become association lists of
/// `(field-name value)`, scalars become atoms. The total-read path a live
/// plotter or script uses. Unsupported leaves become `Void` rather than
/// failing the whole read.
pub fn reflect_to_steel(value: &dyn PartialReflect) -> SteelVal {
    let value = unwrap_newtype(value);
    match value.reflect_ref() {
        ReflectRef::Struct(s) => {
            let pairs: Vec<SteelVal> = (0..s.field_len())
                .filter_map(|i| {
                    let name = s.name_at(i)?;
                    let fv = s.field_at(i)?;
                    Some(SteelVal::ListV(
                        [SteelVal::StringV(name.into()), reflect_to_steel(fv)]
                            .into_iter()
                            .collect(),
                    ))
                })
                .collect();
            SteelVal::ListV(pairs.into_iter().collect())
        }
        _ => scalar_to_steel(value).unwrap_or(SteelVal::Void),
    }
}

/// Read the scalar at a reflect path as `f64`. Generic over the concrete root
/// so the `GetPath` blanket impl applies (`&dyn PartialReflect` does not
/// satisfy it). Returns `None` if the path is missing or the leaf is not a
/// numeric scalar.
pub fn read_path<T: Reflect>(root: &T, path: &str) -> Option<f64> {
    let leaf = unwrap_newtype(resolve(root.as_partial_reflect(), path)?);
    let any = leaf.try_as_reflect()?.as_any();
    any.downcast_ref::<f32>()
        .map(|v| f64::from(*v))
        .or_else(|| any.downcast_ref::<f64>().copied())
        .or_else(|| any.downcast_ref::<u32>().map(|v| f64::from(*v)))
        .or_else(|| any.downcast_ref::<i32>().map(|v| f64::from(*v)))
}

/// Write an `f64` onto the scalar at a reflect path, coercing to the leaf's
/// concrete type (`f32`/`f64`/`u32`/`i32`/`bool`) and descending through a
/// dimensional newtype wrapper (`Mass(f32)`) to its inner scalar. Never names
/// a field, so it works for any reflected type.
pub fn write_path<T: Reflect>(root: &mut T, path: &str, val: f64) -> Result<(), BridgeError> {
    let leaf = resolve_mut(root.as_partial_reflect_mut(), path)
        .ok_or_else(|| BridgeError::Path(path.to_string()))?;
    apply_scalar(leaf, val).map_err(|()| BridgeError::UnsupportedLeaf(path.to_string()))
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

/// Coerce a numeric-ish steel value to `f64` (integer, float, or boolean).
pub fn steel_to_f64(v: &SteelVal) -> Option<f64> {
    match v {
        SteelVal::NumV(n) => Some(*n),
        SteelVal::IntV(i) => Some(*i as f64),
        SteelVal::BoolV(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    // float_cmp: asserted values are set exactly through the bridge.
    #![allow(clippy::float_cmp)]
    use super::*;
    use gradiance_domain::settings::SimSettings;
    use std::sync::{Arc, Mutex};
    use steel::steel_vm::engine::Engine;
    use steel::steel_vm::register_fn::RegisterFn;

    /// A dimensional-newtype stand-in — the exact shape every `gradiance-units`
    /// quantity (`Length`, `Mass`, …) will take: a `#[derive(Reflect)]` newtype
    /// over `f32`.
    #[derive(bevy::reflect::Reflect, Clone, Copy, PartialEq, Debug)]
    struct SpikeMass(f32);

    #[derive(bevy::reflect::Reflect, Clone, Debug)]
    struct SpikeBody {
        mass: SpikeMass,
        count: u32,
    }

    #[test]
    fn dimensional_newtypes_read_and_write_as_scalars() {
        // P0 of the engineering-units pass: a typed quantity newtype must be
        // read-total-native — read, written, and dumped as a plain number, by
        // its field name (no `.0` suffix) and never as `Void`. This is what
        // keeps typed quantities from fighting the scripting reflection bridge.
        let mut body = SpikeBody {
            mass: SpikeMass(2.5),
            count: 3,
        };

        // Read the newtype by plain field name; a bare scalar still reads too.
        assert_eq!(read_path(&body, "mass"), Some(2.5));
        assert_eq!(read_path(&body, "count"), Some(3.0));

        // Write descends through the newtype wrapper.
        write_path(&mut body, "mass", 4.0).expect("newtype writable");
        assert_eq!(body.mass, SpikeMass(4.0));

        // The newtype dumps as its number, not `Void`.
        assert!(matches!(
            reflect_to_steel(&body.mass as &dyn PartialReflect),
            SteelVal::NumV(v) if (v - 4.0).abs() < 1e-9
        ));
        assert!(matches!(
            scalar_to_steel(&body.mass as &dyn PartialReflect),
            Some(SteelVal::NumV(v)) if (v - 4.0).abs() < 1e-9
        ));
    }

    /// A vector-quantity stand-in (`Acceleration2(Vec2)`'s shape).
    #[derive(bevy::reflect::Reflect, Clone, Copy, PartialEq, Debug)]
    struct SpikeAccel(bevy::math::Vec2);

    #[derive(bevy::reflect::Reflect, Clone, Debug)]
    struct SpikeSettings {
        gravity: SpikeAccel,
        lights: Vec<SpikeMass>,
    }

    #[test]
    fn paths_step_through_a_vector_newtype() {
        // `"gravity.y"` must keep resolving when `gravity` goes from a bare
        // `Vec2` to `Acceleration2(Vec2)` (brief 5-1): the path steps through
        // the wrapper, and the explicit `.0` spelling still works.
        let mut settings = SpikeSettings {
            gravity: SpikeAccel(bevy::math::Vec2::new(0.5, -9.75)),
            lights: vec![SpikeMass(1.0), SpikeMass(2.0)],
        };
        assert_eq!(read_path(&settings, "gravity.y"), Some(-9.75));
        assert_eq!(read_path(&settings, "gravity.0.x"), Some(0.5));
        assert_eq!(read_path(&settings, "lights[1]"), Some(2.0));
        write_path(&mut settings, "gravity.y", -3.0).expect("through the newtype");
        write_path(&mut settings, "gravity.0.x", 1.5).expect("explicit .0");
        write_path(&mut settings, "lights[0]", 7.0).expect("indexed newtype");
        assert_eq!(
            settings.gravity,
            SpikeAccel(bevy::math::Vec2::new(1.5, -3.0))
        );
        assert_eq!(settings.lights[0], SpikeMass(7.0));
        // Misses stay misses.
        assert_eq!(read_path(&settings, "gravity.z"), None);
        assert!(write_path(&mut settings, "gravity.z", 0.0).is_err());
        // A vector newtype is not a scalar leaf.
        assert!(write_path(&mut settings, "gravity", 0.0).is_err());
    }

    #[test]
    fn steel_drives_real_sim_settings_through_reflection() {
        let settings = Arc::new(Mutex::new(SimSettings::default()));
        let mut vm = Engine::new();

        let s_get = Arc::clone(&settings);
        vm.register_fn("sim-get", move |path: String| -> f64 {
            read_path(&*s_get.lock().expect("lock"), &path).unwrap_or(f64::NAN)
        });
        let s_set = Arc::clone(&settings);
        vm.register_fn("sim-set", move |path: String, val: SteelVal| {
            let f = steel_to_f64(&val).expect("numeric value");
            write_path(&mut *s_set.lock().expect("lock"), &path, f).expect("path writable");
        });
        let s_dump = Arc::clone(&settings);
        vm.register_fn("sim-dump", move || -> SteelVal {
            reflect_to_steel(&*s_dump.lock().expect("lock") as &dyn PartialReflect)
        });

        // The Rust side never names a field — the script does, resolved by
        // reflection.
        let script = r#"
            (sim-set "speed" (* 2.0 (sim-get "speed")))
            (sim-set "gravity.y" -500.0)
            (sim-set "substeps" 12)
            (sim-dump)
        "#;
        let results = vm.run(script).expect("script runs");

        let final_settings = settings.lock().expect("lock").clone();
        assert_eq!(final_settings.speed, 2.0);
        assert_eq!(final_settings.gravity.value().y, -500.0);
        assert_eq!(final_settings.substeps, 12);
        assert_eq!(final_settings.gravity.value().x, 0.0);
        assert!(matches!(results.last(), Some(SteelVal::ListV(_))));
    }

    #[test]
    fn opaque_custom_type_round_trips() {
        // Local stand-in for the ShapeDef-as-handle path (orphan rule forbids
        // impl'ing steel::Custom on a foreign type from a test; in-crate it is
        // legal for gradiance's own types).
        #[derive(Clone, Debug)]
        struct Shape {
            radius: f32,
        }
        impl steel::rvals::Custom for Shape {}

        let mut vm = Engine::new();
        vm.register_fn("make-circle", |r: f64| -> Shape {
            Shape { radius: r as f32 }
        });
        vm.register_fn("shape-radius", |s: Shape| -> f64 { f64::from(s.radius) });

        let out = vm
            .run("(shape-radius (make-circle 7.0))")
            .expect("opaque round-trips");
        assert!(matches!(out.last(), Some(SteelVal::NumV(v)) if (*v - 7.0).abs() < 1e-9));
    }

    #[test]
    fn bridge_reads_a_real_authored_intent() {
        // Closes the loop from spike #1: the same generic bridge that drove
        // `SimSettings` now reads a real, newly-`Reflect` authored intent —
        // the read-total path the operation registry / live plotters use.
        use bevy::math::Vec2;
        use gradiance_command::intent::SpawnBodyIntent;
        use gradiance_core::ids::StableId;
        use gradiance_core::units::PosRot;
        use gradiance_domain::appearance::Appearance;
        use gradiance_domain::depth::DepthBand;
        use gradiance_domain::props::BodyPhysics;
        use gradiance_domain::shape::ShapeDef;
        use gradiance_scene::BodyRecord;

        let intent = SpawnBodyIntent {
            record: BodyRecord {
                id: StableId::new(),
                pose: PosRot {
                    pos: Vec2::new(12.0, -3.5),
                    rot: 0.0,
                },
                shape: ShapeDef::Circle { radius: 9.0 },
                physics: BodyPhysics::default(),
                appearance: Appearance::default(),
                depth: DepthBand::default(),
                layers: None,
                groups: Vec::new(),
                field: None,
                tracer: None,
            },
        };

        // Reflect-path reads reach the numeric leaves of a real intent,
        // never naming a field on the Rust side.
        assert_eq!(read_path(&intent, "record.pose.pos.x"), Some(12.0));
        // `DepthBand::default().far` is `LAYER_HEIGHT` (0.1 m); the f32 leaf
        // widens to f64 exactly as stored.
        assert_eq!(
            read_path(&intent, "record.depth.far"),
            Some(f64::from(0.1_f32))
        );

        // The whole value converts to steel struct data. Opaque handles
        // (`StableId`, `ShapeDef`) degrade to `Void` by design — they are
        // built via constructor builtins, not read field-by-field.
        let data = reflect_to_steel(&intent as &dyn PartialReflect);
        assert!(matches!(data, SteelVal::ListV(_)));
    }
}
