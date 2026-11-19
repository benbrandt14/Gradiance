//! The quantity catalog end to end (brief 5-2): the `quantity` operations
//! read the same rows the signal bindings and sensor ports read, through
//! every doorway, and a wrong name is a visible error.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, headless_app, paused_app, step};
use avian2d::prelude::RigidBody;
use bevy::prelude::*;
use gradiance::domain::quantity::QUANTITIES;
use gradiance::op::invoke;
use gradiance::prelude::*;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::signal::{SignalBinding, SignalBindings, SignalBus, SignalSink, SignalSource};
use serde_json::{Value, json};

fn spawn(app: &mut App, pos: Vec2, rigid_body: RigidBody) -> StableId {
    let mut record = box_record(pos, 0.2, 0.2);
    record.physics.rigid_body = rigid_body;
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// One operation's returned value.
fn read(app: &mut App, id: &str, params: Value) -> Value {
    invoke(app.world_mut(), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .unwrap_or_else(|| panic!("{id} returned nothing"))
}

fn number(app: &mut App, id: &str, params: Value) -> f32 {
    let value = read(app, id, params);
    #[expect(clippy::cast_possible_truncation, reason = "test tolerance is f32")]
    value.as_f64().map(|v| v as f32).expect("a number")
}

#[test]
fn the_verbs_list_and_read_the_catalog() {
    let mut app = paused_app();
    let a = spawn(&mut app, Vec2::new(0.5, 1.25), RigidBody::Dynamic);
    let b = spawn(&mut app, Vec2::new(0.5, -0.75), RigidBody::Static);
    let (ia, ib) = if a.0 < b.0 { (0, 1) } else { (1, 0) };

    let names = read(&mut app, "quantity.list", json!({}));
    let want: Vec<Value> = QUANTITIES.iter().map(|q| json!(q.name)).collect();
    assert_eq!(names, Value::Array(want));
    assert_eq!(
        read(&mut app, "quantity.unit", json!({"name": "speed"})),
        "m/s"
    );
    assert_eq!(
        read(&mut app, "quantity.unit", json!({"name": "contact-count"})),
        ""
    );

    let height = number(
        &mut app,
        "quantity.read",
        json!({"name": "height", "i": ia}),
    );
    assert!((height - 1.25).abs() < 1e-4, "{height}");
    let x = number(&mut app, "quantity.read", json!({"name": "pos-x", "i": ia}));
    assert!((x - 0.5).abs() < 1e-4, "{x}");
    let gap = number(
        &mut app,
        "quantity.between",
        json!({"name": "distance", "a": ia, "b": ib}),
    );
    assert!((gap - 2.0).abs() < 1e-4, "{gap}");
    // Mass is density × area (0.2 m × 0.2 m).
    let density = number(&mut app, "body.density", json!({"i": ia}));
    let mass = number(&mut app, "quantity.read", json!({"name": "mass", "i": ia}));
    assert!((mass - density * 0.04).abs() < 1e-4, "{mass} vs {density}");
    // A body that is not there reads NaN, as body-x does.
    assert!(read(&mut app, "quantity.read", json!({"name": "speed", "i": 9})).is_null());
}

#[test]
fn a_verb_reads_what_the_plot_binding_reads() {
    let mut app = headless_app();
    let id = spawn(&mut app, Vec2::new(0.0, 2.0), RigidBody::Dynamic);
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "fall".into(),
            source: SignalSource::Speed(id),
            map: default(),
            curve: None,
            gradient: default(),
            sink: SignalSink::Plot,
        });
    step(&mut app, 20);
    // Pause, so the bus and the context read one frozen state.
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    step(&mut app, 2);
    let plotted = app.world().resource::<SignalBus>().get("fall").unwrap();
    let read = number(
        &mut app,
        "quantity.read",
        json!({"name": "speed", "i": id.0.to_string()}),
    );
    assert!(plotted > 1.0, "the body is falling: {plotted}");
    assert!(
        (read - plotted).abs() < 1e-5,
        "verb {read} vs plot {plotted}"
    );
}

#[test]
fn a_joint_quantity_reads_a_joint() {
    let mut app = paused_app();
    let a = spawn(&mut app, Vec2::ZERO, RigidBody::Static);
    let b = spawn(&mut app, Vec2::new(0.5, 0.0), RigidBody::Static);
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Slider {
                    axis: Vec2::X,
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: a,
                body_b: Some(b),
                anchor_a: gradiance::units::Displacement(Vec2::ZERO),
                anchor_b: gradiance::units::Displacement(Vec2::ZERO),
                rest_rot_a: gradiance::units::Angle(0.0),
                rest_rot_b: gradiance::units::Angle(0.0),
            },
        },
    });
    step(&mut app, 2);
    let length = number(
        &mut app,
        "quantity.read",
        json!({"name": "joint-length", "i": 0}),
    );
    assert!((length - 0.5).abs() < 1e-4, "{length}");
    let angle = number(
        &mut app,
        "quantity.read",
        json!({"name": "joint-angle", "i": 0}),
    );
    assert!(angle.abs() < 1e-4, "{angle}");
}

#[test]
fn a_wrong_name_or_verb_is_an_error_that_says_what_to_write() {
    let mut app = paused_app();
    spawn(&mut app, Vec2::ZERO, RigidBody::Dynamic);
    let err = |app: &mut App, id: &str, params: Value| {
        invoke(app.world_mut(), id, params)
            .err()
            .map_or_else(|| panic!("{id} should fail"), |e| e.to_string())
    };
    let unknown = err(
        &mut app,
        "quantity.read",
        json!({"name": "velocity", "i": 0}),
    );
    assert!(unknown.contains("known: speed"), "{unknown}");
    let pair = err(
        &mut app,
        "quantity.read",
        json!({"name": "distance", "i": 0}),
    );
    assert!(pair.contains("quantity-between"), "{pair}");
    let single = err(
        &mut app,
        "quantity.between",
        json!({"name": "speed", "a": 0, "b": 0}),
    );
    assert!(single.contains("(quantity \"speed\" i)"), "{single}");

    // The Lisp doorway shows it in the console log.
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit("(quantity \"velocity\" 0)");
    app.update();
    let log = app
        .world()
        .resource::<ScriptLog>()
        .0
        .last()
        .cloned()
        .unwrap();
    assert!(!log.ok, "{log:?}");
    assert!(log.output.contains("unknown quantity"), "{log:?}");
}
