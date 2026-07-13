//! Signal dataflow: scene attributes → bus → color/plot sinks
//! (`docs/signal-dataflow.md`).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, paused_app, step};
use avian2d::prelude::{LinearVelocity, RigidBody};
use bevy::prelude::*;
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
            gradient: default(),
            sink: SignalSink::Fill(id),
        },
    );
    let entity = entity_of(&app, id).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .insert(LinearVelocity(Vec2::new(100.0, 0.0)));
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
    app.world_mut()
        .entity_mut(entity)
        .insert(LinearVelocity(Vec2::new(390.0, 0.0)));
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
    let falling = box_record(Vec2::new(0.0, 120.0), 20.0, 20.0);
    let falling_id = falling.id;
    let mut floor = box_record(Vec2::new(0.0, -100.0), 1000.0, 20.0);
    floor.physics.rigid_body = RigidBody::Static;
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
    use gradiance::persist::{from_ron, to_ron};
    let mut record = gradiance::command::snapshot::SceneRecord {
        version: gradiance::persist::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 10.0, 10.0)],
        joints: vec![],
        environment: gradiance::command::snapshot::EnvironmentRecord::default(),
    };
    let id = record.bodies[0].id;
    record.environment.signals.0.push(SignalBinding {
        name: "speed".into(),
        source: SignalSource::Speed(id),
        map: default(),
        gradient: gradiance::signal::GradientSpec::Turbo,
        sink: SignalSink::Fill(id),
    });

    let text = to_ron(&record).unwrap();
    let parsed = from_ron(&text).expect("bindings round-trip");
    assert_eq!(parsed.environment.signals, record.environment.signals);

    // Pre-signal files (no `signals` field) still parse.
    let plain = gradiance::command::snapshot::SceneRecord {
        environment: gradiance::command::snapshot::EnvironmentRecord::default(),
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
