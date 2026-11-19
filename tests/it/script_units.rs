//! Units in scripts end to end (brief 5-4): `q`, `convert` and `unit-of`
//! convert through the typed-field parser, a bad unit stops the run with a
//! visible error, and the mixed-unit scene in `docs/scripting.md` spawns the
//! same bodies as its SI twin.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::paused_app;
use bevy::prelude::*;
use gradiance::domain::Body;
use gradiance::geometry::sdf::aabb;
use gradiance::op::invoke;
use gradiance::scene::BodyRecord;
use gradiance::script::bridge::{ScriptEntry, ScriptInputs, ScriptLog};
use gradiance::units::expr::evaluate;
use serde_json::{Value, json};

fn read(app: &mut App, id: &str, params: Value) -> Value {
    invoke(app.world_mut(), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .unwrap_or_else(|| panic!("{id} returned nothing"))
}

fn number(app: &mut App, id: &str, params: Value) -> f64 {
    read(app, id, params).as_f64().expect("a number")
}

/// Submits `source`, runs it, and returns its log entries.
fn run(app: &mut App, source: &str) -> Vec<ScriptEntry> {
    let before = app.world().resource::<ScriptLog>().0.len();
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
    app.update();
    app.world().resource::<ScriptLog>().0[before..].to_vec()
}

fn bodies(app: &mut App) -> Vec<BodyRecord> {
    let entities: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<Body>>()
        .iter(app.world())
        .collect();
    entities
        .into_iter()
        .filter_map(|e| BodyRecord::capture(app.world(), e))
        .collect()
}

/// The conversions agree with the field parser reading the same text, and
/// round-trip.
#[test]
fn conversions_round_trip_through_the_field_parser() {
    let mut app = paused_app();
    for (value, unit) in [
        (25.0, "cm"),
        (90.0, "deg"),
        (2.0, "kN"),
        (36.0, "km/h"),
        (120.0, "rpm"),
        (3.0, "ft"),
        (5.0, "N m"),
    ] {
        let si = number(&mut app, "units.si", json!({"value": value, "unit": unit}));
        let typed = evaluate(&format!("{value} {unit}")).unwrap();
        assert!(
            (si - typed.si).abs() < 1e-12,
            "{value} {unit}: {si} vs {}",
            typed.si
        );
        let symbol = read(&mut app, "units.of", json!({"unit": unit}));
        let back = number(
            &mut app,
            "units.convert",
            json!({"value": si, "from": symbol, "to": unit}),
        );
        assert!(
            (back - value).abs() < 1e-9,
            "{value} {unit} came back {back}"
        );
    }
    let speed = number(
        &mut app,
        "units.convert",
        json!({"value": 90, "from": "km/h", "to": "speed"}),
    );
    assert!((speed - 25.0).abs() < 1e-9, "{speed}");
    // unit-of speaks quantity-unit's symbols.
    for name in [
        "speed",
        "kinetic-energy",
        "height",
        "joint-angle",
        "contact-count",
    ] {
        let unit = read(&mut app, "quantity.unit", json!({"name": name}));
        if unit != "" {
            assert_eq!(
                read(&mut app, "units.of", json!({"unit": unit})),
                unit,
                "{name}"
            );
        }
    }
}

/// The Lisp doorway: the verbs run in a script and feed a spawn.
#[test]
fn a_script_spawns_in_mixed_units() {
    let mut app = paused_app();
    let log = run(
        &mut app,
        "(begin (define (cm x) (q x \"cm\"))
                (spawn-box (cm 50) (convert 25 \"cm\" \"m\") (cm 40) (q 300 \"mm\")))",
    );
    assert!(log.iter().all(|e| e.ok), "{log:?}");
    let all = bodies(&mut app);
    assert_eq!(all.len(), 1);
    let body = &all[0];
    assert!(
        (body.pose.pos - Vec2::new(0.5, 0.25)).length() < 1e-5,
        "{:?}",
        body.pose
    );
    let (min, max) = aabb(&body.shape);
    assert!(
        ((max - min) - Vec2::new(0.4, 0.3)).length() < 1e-5,
        "{min} {max}"
    );
}

/// A unit the parser does not read, and a conversion between dimensions,
/// stop the run with an error that says why; nothing is spawned.
#[test]
fn a_bad_unit_is_a_visible_script_error() {
    for (source, says) in [
        (
            "(spawn-box 0 0 (q 1 \"furlong\") 1)",
            "unknown unit `furlong`",
        ),
        (
            "(spawn-box 0 0 (convert 90 \"deg\" \"height\") 1)",
            "expected m, got rad",
        ),
        (
            "(spawn-box 0 0 (convert 1 \"m\" \"s\") 1)",
            "expected s, got m",
        ),
        ("(unit-of \"cm cm +\")", "unit \"cm cm +\""),
    ] {
        let mut app = paused_app();
        let log = run(&mut app, source);
        let last = log
            .last()
            .unwrap_or_else(|| panic!("{source}: no log entry"));
        assert!(!last.ok, "{source}: {last:?}");
        assert!(last.output.contains(says), "{source}: {}", last.output);
        assert!(bodies(&mut app).is_empty(), "{source} spawned");
    }
}

/// The fenced `scheme` block of `docs/scripting.md` whose first line is
/// `marker`.
fn doc_block(marker: &str) -> String {
    let doc = include_str!("../../docs/scripting.md");
    let start = doc
        .find(&format!("```scheme\n{marker}\n"))
        .unwrap_or_else(|| panic!("no block `{marker}` in docs/scripting.md"));
    let body = &doc[start + "```scheme\n".len()..];
    body[..body.find("```").expect("a closed block")].to_owned()
}

/// Every body as (shape bounds, position, rotation), in a stable order.
fn scene_of(source: &str) -> Vec<(Vec2, Vec2, Vec2, f32)> {
    let mut app = paused_app();
    let log = run(&mut app, source);
    assert!(log.iter().all(|e| e.ok), "{log:?}");
    let mut scene: Vec<_> = bodies(&mut app)
        .iter()
        .map(|b| {
            let (min, max) = aabb(&b.shape);
            (min, max, b.pose.pos, b.pose.rot)
        })
        .collect();
    scene.sort_by(|a, b| a.2.x.total_cmp(&b.2.x).then(a.2.y.total_cmp(&b.2.y)));
    scene
}

#[test]
fn the_mixed_unit_doc_example_spawns_its_si_twin() {
    let mixed = scene_of(&doc_block(";; units: mixed"));
    let si = scene_of(&doc_block(";; units: SI"));
    assert_eq!(mixed.len(), 4, "{mixed:?}");
    assert_eq!(mixed.len(), si.len());
    for (a, b) in mixed.iter().zip(&si) {
        let near = |p: Vec2, q: Vec2| {
            p.is_finite() == q.is_finite() && (!p.is_finite() || (p - q).length() < 1e-5)
        };
        assert!(near(a.0, b.0) && near(a.1, b.1), "bounds {a:?} vs {b:?}");
        assert!((a.2 - b.2).length() < 1e-5, "position {a:?} vs {b:?}");
        assert!((a.3 - b.3).abs() < 1e-6, "rotation {a:?} vs {b:?}");
    }
}
