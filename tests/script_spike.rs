//! Spike 1: the `bevy_reflect` <-> steel bridge, exercised inside the real
//! crate against gradiance's actual `SimSettings` (which already derives
//! `Reflect`).
//!
//! This is a **spike**, deliberately an integration test with steel as a
//! dev-dependency: it validates the load-bearing assumption behind the
//! scripting design (see `docs/script-lisp-decision.md`) — that ONE generic,
//! reflection-driven bridge makes every `#[derive(Reflect)]` type scriptable
//! with no per-field Rust code — *without* committing steel as a shipping
//! dependency before P0. At P0 the bridge moves into `src/script/` as
//! product code and steel becomes a (feature-gated) real dependency.
//!
//! What it proves:
//! - a steel script mutates a real Rust value through reflect paths
//!   (`speed`: f32, nested `gravity.y`: glam `Vec2`, `substeps`: u32 via
//!   numeric coercion), leaving untouched fields intact;
//! - the whole value round-trips to steel data ("reads are total");
//! - opaque custom types round-trip (the `ShapeDef`-as-handle path).

// Integration-test file: panics/unwraps are the failure mechanism, matching
// the convention in tests/boundaries.rs.
// float_cmp: the asserted values are set exactly through the bridge, so exact
// equality is the correct check. manual_map: the leaf type-probe cascades read
// clearer as if/else-if than as chained `.map().or_else()`.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::manual_map,
    clippy::cast_possible_wrap
)]

use bevy::reflect::{GetPath, PartialReflect, Reflect, ReflectRef};
use gradiance::domain::settings::SimSettings;
use std::sync::{Arc, Mutex};
use steel::rvals::SteelVal;
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;

// ---- the generic bridge (type-agnostic: only reflection, never field names) ----

/// A reflected leaf scalar -> a steel value.
fn scalar_to_steel(leaf: &dyn PartialReflect) -> Option<SteelVal> {
    let any = leaf.try_as_reflect()?.as_any();
    if let Some(v) = any.downcast_ref::<f32>() {
        Some(SteelVal::NumV(f64::from(*v)))
    } else if let Some(v) = any.downcast_ref::<f64>() {
        Some(SteelVal::NumV(*v))
    } else if let Some(v) = any.downcast_ref::<u32>() {
        Some(SteelVal::IntV(*v as isize))
    } else if let Some(v) = any.downcast_ref::<i32>() {
        Some(SteelVal::IntV(*v as isize))
    } else if let Some(v) = any.downcast_ref::<bool>() {
        Some(SteelVal::BoolV(*v))
    } else {
        None
    }
}

/// Whole reflected value -> steel data (structs become assoc lists). The
/// "reads are total" path a live plotter/script would use.
fn reflect_to_steel(value: &dyn PartialReflect) -> SteelVal {
    match value.reflect_ref() {
        ReflectRef::Struct(s) => {
            let pairs: Vec<SteelVal> = (0..s.field_len())
                .map(|i| {
                    let name = s.name_at(i).unwrap_or("?");
                    let fv = s.field_at(i).expect("field exists");
                    SteelVal::ListV(
                        [SteelVal::StringV(name.into()), reflect_to_steel(fv)]
                            .into_iter()
                            .collect(),
                    )
                })
                .collect();
            SteelVal::ListV(pairs.into_iter().collect())
        }
        _ => scalar_to_steel(value).unwrap_or(SteelVal::Void),
    }
}

/// Read a scalar at a reflect path as f64. Generic over the concrete root so
/// the `GetPath` blanket impl applies (a `&dyn PartialReflect` would not).
fn read_path<T: Reflect>(root: &T, path: &str) -> Option<f64> {
    let any = root.reflect_path(path).ok()?.try_as_reflect()?.as_any();
    if let Some(v) = any.downcast_ref::<f32>() {
        Some(f64::from(*v))
    } else if let Some(v) = any.downcast_ref::<f64>() {
        Some(*v)
    } else if let Some(v) = any.downcast_ref::<u32>() {
        Some(f64::from(*v))
    } else if let Some(v) = any.downcast_ref::<i32>() {
        Some(f64::from(*v))
    } else {
        None
    }
}

/// Write an f64 onto whatever scalar type lives at a reflect path, coercing to
/// the target's concrete type by trying each candidate apply. Never names a
/// field.
fn write_path<T: Reflect>(root: &mut T, path: &str, val: f64) -> Result<(), String> {
    let leaf = root.reflect_path_mut(path).map_err(|e| format!("{e:?}"))?;
    if leaf.try_apply(&(val as f32) as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&val as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&(val as u32) as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&(val as i32) as &dyn PartialReflect).is_ok()
        || leaf.try_apply(&(val != 0.0) as &dyn PartialReflect).is_ok()
    {
        Ok(())
    } else {
        Err(format!("unsupported leaf type at {path}"))
    }
}

/// Coerce a numeric-ish steel value to f64 (int, float, or bool).
fn steel_to_f64(v: &SteelVal) -> Option<f64> {
    match v {
        SteelVal::NumV(n) => Some(*n),
        SteelVal::IntV(i) => Some(*i as f64),
        SteelVal::BoolV(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
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

    // The Rust side above never mentions "speed"/"gravity"/"substeps" — the
    // script names the fields, resolved entirely by reflection.
    let script = r#"
        (sim-set "speed" (* 2.0 (sim-get "speed")))
        (sim-set "gravity.y" -500.0)
        (sim-set "substeps" 12)
        (sim-dump)
    "#;
    let results = vm.run(script).expect("script runs");

    let final_settings = settings.lock().expect("lock").clone();
    assert_eq!(final_settings.speed, 2.0, "f32 field doubled via path");
    assert_eq!(final_settings.gravity.y, -500.0, "nested Vec2 field set");
    assert_eq!(final_settings.substeps, 12, "u32 field set via coercion");
    assert_eq!(final_settings.gravity.x, 0.0, "untouched field preserved");

    // The total-read dump is well-formed nested steel data.
    match results.last() {
        Some(SteelVal::ListV(_)) => {}
        other => panic!("expected assoc-list dump, got {other:?}"),
    }
}

#[test]
fn opaque_custom_type_round_trips() {
    // Stand-in for the ShapeDef-as-handle path (a local type, since the
    // orphan rule forbids `impl steel::Custom for gradiance::ShapeDef` here;
    // in-crate at P0 that impl is legal).
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
    assert!(
        matches!(out.last(), Some(SteelVal::NumV(v)) if (*v - 7.0).abs() < 1e-9),
        "opaque handle carried a Rust value through steel: {:?}",
        out.last()
    );
}
