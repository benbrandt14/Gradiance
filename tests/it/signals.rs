//! Signal dataflow: scene attributes → bus → color/plot sinks
//! (`docs/signal-dataflow.md`).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, paused_app, step};
use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;
use gradiance::prelude::*;
use gradiance::signal::{
    ScriptSignals, SignalBinding, SignalBindings, SignalBus, SignalColorOverride, SignalMap,
    SignalSink, SignalSource,
};

fn bind(app: &mut App, binding: SignalBinding) {
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(binding);
}

fn bus_value(app: &App, name: &str) -> Option<f32> {
    app.world().resource::<SignalBus>().get(name)
}

#[test]
fn a_speed_binding_tints_the_body_and_cleans_up_when_removed() {
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();
    let record = box_record(Vec2::ZERO, 20.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    bind(
        &mut app,
        SignalBinding {
            name: "speed".into(),
            source: SignalSource::Speed(id),
            map: SignalMap {
                in_min: 0.0,
                in_max: 400.0,
            },
            curve: None,
            gradient: default(),
            sink: SignalSink::Fill(id),
        },
    );
    let entity = entity_of(&app, id).unwrap();
    app.world_mut().entity_mut(entity).insert(Velocity {
        linear: Vec3::new(100.0, 0.0, 0.0),
        angular: Vec3::ZERO,
    });
    step(&mut app, 3);

    let slow_tint = app
        .world()
        .get::<SignalColorOverride>(entity)
        .expect("the binding tinted the body")
        .fill
        .expect("fill sink writes the fill tint");
    let slow_value = bus_value(&app, "speed").expect("published on the bus");
    assert!(
        (slow_value - 100.0).abs() < 5.0,
        "speed ≈ 100 ({slow_value})"
    );

    // Faster → a different band of the gradient.
    app.world_mut().entity_mut(entity).insert(Velocity {
        linear: Vec3::new(390.0, 0.0, 0.0),
        angular: Vec3::ZERO,
    });
    step(&mut app, 3);
    let fast_tint = app
        .world()
        .get::<SignalColorOverride>(entity)
        .unwrap()
        .fill
        .unwrap();
    assert_ne!(slow_tint, fast_tint, "the tint tracks the signal");

    // Removing the binding removes the override (authored fill returns)
    // and its bus entry.
    app.world_mut().resource_mut::<SignalBindings>().0.clear();
    step(&mut app, 2);
    let entity = entity_of(&app, id).unwrap();
    assert!(
        app.world().get::<SignalColorOverride>(entity).is_none(),
        "override removed with its binding"
    );
    assert_eq!(bus_value(&app, "speed"), None, "bus entry dropped");
}

#[test]
fn distance_and_named_sources_publish_and_drive() {
    let mut app = paused_app();
    let a = box_record(Vec2::new(-100.0, 0.0), 20.0, 20.0);
    let b = box_record(Vec2::new(140.0, 0.0), 20.0, 20.0);
    let (ida, idb) = (a.id, b.id);
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();

    bind(
        &mut app,
        SignalBinding {
            name: "gap".into(),
            source: SignalSource::Distance(ida, idb),
            map: default(),
            curve: None,
            gradient: default(),
            sink: SignalSink::Plot,
        },
    );
    // A named signal (as a script would publish) driving a fill.
    bind(
        &mut app,
        SignalBinding {
            name: "excitement-fill".into(),
            source: SignalSource::Named("excitement".into()),
            map: SignalMap {
                in_min: 0.0,
                in_max: 10.0,
            },
            curve: None,
            gradient: default(),
            sink: SignalSink::Fill(ida),
        },
    );
    app.world_mut()
        .resource_mut::<ScriptSignals>()
        .0
        .push("excitement".into());
    app.world_mut()
        .resource_mut::<SignalBus>()
        .publish("excitement", 7.0, false);
    step(&mut app, 2);

    let gap = bus_value(&app, "gap").unwrap();
    assert!((gap - 240.0).abs() < 1.0, "distance published ({gap})");
    let entity = entity_of(&app, ida).unwrap();
    assert!(
        app.world()
            .get::<SignalColorOverride>(entity)
            .is_some_and(|o| o.fill.is_some()),
        "the named signal drives the fill"
    );
    assert_eq!(
        bus_value(&app, "excitement"),
        Some(7.0),
        "script-published names survive bus hygiene"
    );
}

#[test]
fn contact_count_source_reads_the_facade() {
    let mut app = headless_app();
    let falling = box_record(Vec2::new(0.0, 1.2), 0.2, 0.2);
    let falling_id = falling.id;
    let mut floor = box_record(Vec2::new(0.0, -1.0), 10.0, 0.2);
    floor.physics.kind = BodyKind::Static;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: falling });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: floor });
    app.update();
    bind(
        &mut app,
        SignalBinding {
            name: "touches".into(),
            source: SignalSource::ContactCount(falling_id),
            map: default(),
            curve: None,
            gradient: default(),
            sink: SignalSink::Plot,
        },
    );

    step(&mut app, 180); // land and rest
    assert!(
        bus_value(&app, "touches").unwrap() >= 1.0,
        "a resting box touches the floor"
    );
}

#[test]
fn bindings_persist_with_the_scene_and_old_files_parse() {
    use gradiance::scene::{from_ron, to_ron};
    let mut record = gradiance::scene::SceneRecord {
        version: gradiance::scene::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 10.0, 10.0)],
        joints: vec![],
        nodes: vec![],
        environment: gradiance::scene::EnvironmentRecord::default(),
    };
    let id = record.bodies[0].id;
    record.environment.signals.0.push(SignalBinding {
        name: "speed".into(),
        source: SignalSource::Speed(id),
        map: default(),
        curve: None,
        gradient: gradiance::signal::GradientSpec::Turbo,
        sink: SignalSink::Fill(id),
    });

    let text = to_ron(&record).unwrap();
    let parsed = from_ron(&text).expect("bindings round-trip");
    assert_eq!(parsed.environment.signals, record.environment.signals);

    // Pre-signal files (no `signals` field) still parse.
    let plain = gradiance::scene::SceneRecord {
        environment: gradiance::scene::EnvironmentRecord::default(),
        ..record
    };
    let text = to_ron(&plain).unwrap();
    let stripped = text.replace("signals: (([])),", "");
    let cut = if stripped == text {
        text.replace("signals:", "legacy_ignore:")
    } else {
        stripped
    };
    assert_ne!(text, cut, "fixture actually removed the signals field");
    let parsed = from_ron(&cut).expect("pre-signal file parses");
    assert!(parsed.environment.signals.0.is_empty());
}

#[test]
fn a_param_publishes_and_a_computed_signal_modulates_it() {
    use gradiance::signal::{
        ComputedSignal, ComputedSignals, SignalExpr, SignalParam, SignalParams,
    };
    let mut app = paused_app();
    // A param "amp" = 3, and a computed "scaled" = amp * 2.
    app.world_mut()
        .resource_mut::<SignalParams>()
        .0
        .push(SignalParam {
            name: "amp".into(),
            value: 3.0,
            min: 0.0,
            max: 10.0,
        });
    app.world_mut()
        .resource_mut::<ComputedSignals>()
        .0
        .push(ComputedSignal {
            name: "scaled".into(),
            expr: SignalExpr::parse_rpn("amp 2 *").unwrap(),
            block: None,
        });
    step(&mut app, 2);

    assert_eq!(bus_value(&app, "amp"), Some(3.0), "param published");
    assert_eq!(bus_value(&app, "scaled"), Some(6.0), "computed = amp * 2");

    // Turning the knob re-modulates next frame.
    app.world_mut().resource_mut::<SignalParams>().0[0].value = 5.0;
    step(&mut app, 2);
    assert_eq!(bus_value(&app, "scaled"), Some(10.0), "tracks the param");
}

#[test]
fn a_body_sensor_feeds_a_modulation_block_over_the_canonical_bus_name() {
    use gradiance::signal::{ComputedSignal, ComputedSignals, SignalExpr};
    // The node canvas wires a body's speed output into a modulation block by
    // referencing the body's canonical sensor bus name ("speed@<uuid>"). Only
    // `publish_sensor_refs` puts that name on the bus — no binding sinks it —
    // so this is the end-to-end proof the sensor→modulation seam is live.
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();
    let record = box_record(Vec2::ZERO, 20.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    // Give the body a known, constant speed (zero gravity, no drag).
    let entity = entity_of(&app, id).unwrap();
    app.world_mut().entity_mut(entity).insert(Velocity {
        linear: Vec3::new(30.0, 40.0, 0.0),
        angular: Vec3::ZERO,
    });

    // "twice-speed" = speed@<id> * 2, referencing the sensor by its bus name.
    let sensor = SignalSource::Speed(id).bus_name().unwrap();
    app.world_mut()
        .resource_mut::<ComputedSignals>()
        .0
        .push(ComputedSignal {
            name: "twice-speed".into(),
            expr: SignalExpr::Mul(
                Box::new(SignalExpr::Input(sensor.clone())),
                Box::new(SignalExpr::Const(2.0)),
            ),
            block: None,
        });
    step(&mut app, 2);

    // publish_sensor_refs surfaced the sensor, and the computed read it.
    assert_eq!(
        bus_value(&app, &sensor),
        Some(50.0),
        "publish_sensor_refs put the body's speed on the canonical bus name"
    );
    assert_eq!(
        bus_value(&app, "twice-speed"),
        Some(100.0),
        "the modulation block read the sensor and doubled it"
    );
}

#[test]
fn a_computed_signal_drives_a_body_color_through_a_named_binding() {
    use gradiance::signal::{
        ComputedSignal, ComputedSignals, SignalBinding, SignalExpr, SignalMap, SignalParam,
        SignalParams, SignalSink, SignalSource,
    };
    let mut app = paused_app();
    let record = box_record(Vec2::ZERO, 20.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    app.world_mut()
        .resource_mut::<SignalParams>()
        .0
        .push(SignalParam {
            name: "heat".into(),
            value: 0.8,
            min: 0.0,
            max: 1.0,
        });
    // computed "warm" = heat (identity modulator, exercising the kernel).
    app.world_mut()
        .resource_mut::<ComputedSignals>()
        .0
        .push(ComputedSignal {
            name: "warm".into(),
            expr: SignalExpr::parse_rpn("heat").unwrap(),
            block: None,
        });
    bind(
        &mut app,
        SignalBinding {
            name: "warm-fill".into(),
            source: SignalSource::Named("warm".into()),
            map: SignalMap {
                in_min: 0.0,
                in_max: 1.0,
            },
            curve: None,
            gradient: default(),
            sink: SignalSink::Fill(id),
        },
    );
    step(&mut app, 2);

    let entity = entity_of(&app, id).unwrap();
    assert!(
        app.world()
            .get::<SignalColorOverride>(entity)
            .is_some_and(|o| o.fill.is_some()),
        "the computed signal drives the fill through the named binding"
    );
    assert_eq!(bus_value(&app, "warm"), Some(0.8));
}

#[test]
fn params_and_computed_persist_and_old_files_parse() {
    use gradiance::scene::{from_ron, to_ron};
    use gradiance::signal::{ComputedSignal, SignalExpr, SignalParam};
    let mut record = gradiance::scene::SceneRecord {
        version: gradiance::scene::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![],
        joints: vec![],
        nodes: vec![],
        environment: gradiance::scene::EnvironmentRecord::default(),
    };
    record.environment.params.0.push(SignalParam::unit("amp"));
    record.environment.computed.0.push(ComputedSignal {
        name: "osc".into(),
        expr: SignalExpr::parse_rpn("t sin amp *").unwrap(),
        block: None,
    });

    let text = to_ron(&record).unwrap();
    let parsed = from_ron(&text).expect("round-trips");
    assert_eq!(parsed.environment.params, record.environment.params);
    assert_eq!(parsed.environment.computed, record.environment.computed);

    // A pre-P2 file (no params/computed fields) still parses. Serialize an
    // empty-environment record and strip the two default fields out.
    let plain = gradiance::scene::SceneRecord {
        environment: gradiance::scene::EnvironmentRecord::default(),
        ..record
    };
    let text = to_ron(&plain).unwrap();
    let cut = text
        .replace("params: ([]),", "")
        .replace("computed: ([]),", "");
    assert_ne!(text, cut, "fixture removed the fields");
    let parsed = from_ron(&cut).expect("pre-P2 file parses");
    assert!(parsed.environment.params.0.is_empty());
    assert!(parsed.environment.computed.0.is_empty());
}
