//! Behavior nodes: the placeable tracer tool, node selection/delete, and
//! behavior-copies-on-duplicate (`docs/signal-dataflow.md`).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::intent::{DuplicateIntent, SpawnNodeIntent};
use gradiance::command::snapshot::NodeRecord;
use gradiance::core::ids::StableId;
use gradiance::core::units::PosRot;
use gradiance::domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance::domain::tracer::Tracer;
use gradiance::interaction::selection::Selection;
use gradiance::prelude::*;

fn tracer_record(pos: Vec2, attach: NodeAttachment) -> NodeRecord {
    let id = StableId::new();
    NodeRecord {
        id,
        pose: PosRot { pos, rot: 0.0 },
        attachment: attach,
        kind: NodeKind::Tracer(Tracer {
            fade_secs: 2.0,
            ..Default::default()
        }),
        appearance: gradiance::domain::appearance::Appearance::default(),
    }
}

fn node_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<BehaviorNode>>()
        .iter(app.world())
        .count()
}

#[test]
fn spawning_a_tracer_node_is_undoable_and_persists() {
    let mut app = paused_app();
    let record = tracer_record(Vec2::new(30.0, 10.0), NodeAttachment::free());
    let id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();

    assert_eq!(node_count(&mut app), 1);
    let entity = entity_of(&app, id).unwrap();
    assert!(
        app.world().get::<Tracer>(entity).is_some(),
        "tracer carried"
    );
    assert!(app.world().get::<NodeKind>(entity).is_some());

    undo(&mut app);
    assert_eq!(node_count(&mut app), 0, "undo removes the node");
}

#[test]
fn an_attached_node_follows_its_body() {
    let mut app = paused_app();
    let mut body = box_record(Vec2::new(100.0, 0.0), 20.0, 20.0);
    body.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    // A node attached at offset (5, 0) in the body's frame.
    let record = tracer_record(
        Vec2::new(105.0, 0.0),
        NodeAttachment::to(body_id, Vec2::new(5.0, 0.0)),
    );
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    step(&mut app, 2);

    // Move the body; the follow system repositions the node.
    let body_entity = entity_of(&app, body_id).unwrap();
    app.world_mut()
        .get_mut::<Transform>(body_entity)
        .unwrap()
        .translation = Vec3::new(-50.0, 40.0, 0.0);
    step(&mut app, 2);
    let node_pos = app
        .world()
        .get::<Transform>(entity_of(&app, node_id).unwrap())
        .unwrap()
        .translation
        .truncate();
    assert!(
        (node_pos - Vec2::new(-45.0, 40.0)).length() < 1e-3,
        "node rides the body at its offset ({node_pos})"
    );
}

#[test]
fn a_node_is_individually_selectable_and_deletable() {
    let mut app = paused_app();
    let record = tracer_record(Vec2::new(0.0, 0.0), NodeAttachment::free());
    let node_id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    let node_entity = entity_of(&app, node_id).unwrap();

    // Select the node directly (the pick system is exercised in ui tests;
    // here we drive the selection + delete command seam).
    app.world_mut().resource_mut::<Selection>().set(node_entity);
    step(&mut app, 1);
    assert!(
        app.world().resource::<Selection>().contains(node_entity),
        "the node stays selected (prune keeps nodes)"
    );

    // Delete removes it, undo restores it (same id).
    app.world_mut().write_message(DeleteIntent {
        targets: vec![node_id],
    });
    app.update();
    assert_eq!(node_count(&mut app), 0);
    undo(&mut app);
    assert_eq!(node_count(&mut app), 1);
    assert!(entity_of(&app, node_id).is_some());
}

#[test]
fn duplicating_a_body_copies_its_attached_tracer_and_bindings() {
    use gradiance::signal::{SignalBinding, SignalBindings, SignalMap, SignalSink, SignalSource};
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 20.0, 20.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    // A tracer attached to the body, and a speed→fill binding about it.
    app.world_mut().write_message(SpawnNodeIntent {
        record: tracer_record(Vec2::ZERO, NodeAttachment::to(body_id, Vec2::ZERO)),
    });
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "speed".into(),
            source: SignalSource::Speed(body_id),
            map: SignalMap::default(),
            gradient: gradiance::signal::GradientSpec::default(),
            sink: SignalSink::Fill(body_id),
        });
    app.update();
    assert_eq!(node_count(&mut app), 1);

    // Duplicate the body.
    app.world_mut().write_message(DuplicateIntent {
        sources: vec![body_id],
        offset: Vec2::new(200.0, 0.0),
    });
    app.update();

    // The attached tracer was cloned onto the duplicate.
    assert_eq!(node_count(&mut app), 2, "the tracer copied with the body");
    // And a binding referencing the clone exists.
    assert_eq!(
        app.world().resource::<SignalBindings>().0.len(),
        2,
        "the binding copied with the body"
    );
    // The clone is the body that isn't the original.
    let ids: Vec<StableId> = app
        .world_mut()
        .query_filtered::<&StableId, With<gradiance::domain::Body>>()
        .iter(app.world())
        .copied()
        .collect();
    let clone_body: StableId = ids.into_iter().find(|id| *id != body_id).unwrap();
    assert!(
        app.world()
            .resource::<SignalBindings>()
            .0
            .iter()
            .any(|b| matches!(b.source, SignalSource::Speed(id) if id == clone_body)),
        "the copied binding points at the duplicate"
    );

    // Undo removes the clones — bodies, nodes, and the copied binding.
    undo(&mut app);
    assert_eq!(node_count(&mut app), 1, "undo removes the cloned tracer");
    assert_eq!(
        app.world().resource::<SignalBindings>().0.len(),
        1,
        "undo removes the copied binding"
    );
}

#[test]
fn a_sensor_publishes_and_an_actuator_consumes_over_the_bus() {
    use avian2d::prelude::LinearVelocity;
    use gradiance::domain::node::{NodeKind, SensorQuantity};
    use gradiance::domain::signal::{GradientSpec, SignalMap};
    use gradiance::signal::{SignalBus, SignalColorOverride};

    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();

    // Body A moves; a sensor on it publishes its speed under "wire".
    let mut a = box_record(Vec2::ZERO, 20.0, 20.0);
    a.physics.rigid_body = avian2d::prelude::RigidBody::Dynamic;
    let a_id = a.id;
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    // Body B is tinted by an actuator reading "wire".
    let b = box_record(Vec2::new(200.0, 0.0), 20.0, 20.0);
    let b_id = b.id;
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();

    let mut sensor = tracer_record(Vec2::ZERO, NodeAttachment::to(a_id, Vec2::ZERO));
    sensor.kind = NodeKind::Sensor {
        quantity: SensorQuantity::Speed,
        signal: "wire".into(),
    };
    app.world_mut()
        .write_message(SpawnNodeIntent { record: sensor });
    let mut actuator = tracer_record(Vec2::new(200.0, 0.0), NodeAttachment::to(b_id, Vec2::ZERO));
    actuator.kind = NodeKind::Actuator {
        signal: "wire".into(),
        map: SignalMap {
            in_min: 0.0,
            in_max: 200.0,
        },
        gradient: GradientSpec::Turbo,
        target: gradiance::domain::node::ActuatorTarget::Fill,
    };
    app.world_mut()
        .write_message(SpawnNodeIntent { record: actuator });
    app.update();

    // Drive A at 150 px/s; the sensor publishes it, the actuator tints B.
    let a_entity = entity_of(&app, a_id).unwrap();
    app.world_mut()
        .entity_mut(a_entity)
        .insert(LinearVelocity(Vec2::new(150.0, 0.0)));
    step(&mut app, 3);

    let speed = app.world().resource::<SignalBus>().get("wire").unwrap();
    assert!(
        (speed - 150.0).abs() < 10.0,
        "the sensor published A's speed ({speed})"
    );
    let b_entity = entity_of(&app, b_id).unwrap();
    assert!(
        app.world()
            .get::<SignalColorOverride>(b_entity)
            .is_some_and(|o| o.fill.is_some()),
        "the actuator tinted B from the wired signal"
    );
}

#[test]
fn an_actuator_can_drive_the_tracer_trail_from_pos_x() {
    use gradiance::domain::node::{ActuatorTarget, NodeKind, SensorQuantity};
    use gradiance::domain::signal::{GradientSpec, SignalMap};
    use gradiance::signal::{SignalBus, SignalColorOverride};

    let mut app = paused_app();
    // A static body at x = 300; a PosX sensor publishes it under "px".
    let mut body = box_record(Vec2::new(300.0, 0.0), 20.0, 20.0);
    body.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();

    let mut sensor = tracer_record(
        Vec2::new(300.0, 0.0),
        NodeAttachment::to(body_id, Vec2::ZERO),
    );
    sensor.kind = NodeKind::Sensor {
        quantity: SensorQuantity::PosX,
        signal: "px".into(),
    };
    app.world_mut()
        .write_message(SpawnNodeIntent { record: sensor });
    // An actuator on the same body reads "px" and drives the *trail* channel.
    let mut actuator = tracer_record(
        Vec2::new(300.0, 0.0),
        NodeAttachment::to(body_id, Vec2::ZERO),
    );
    actuator.kind = NodeKind::Actuator {
        signal: "px".into(),
        map: SignalMap {
            in_min: 0.0,
            in_max: 500.0,
        },
        gradient: GradientSpec::Turbo,
        target: ActuatorTarget::TracerColor,
    };
    app.world_mut()
        .write_message(SpawnNodeIntent { record: actuator });
    step(&mut app, 3);

    let px = app.world().resource::<SignalBus>().get("px").unwrap();
    assert!(
        (px - 300.0).abs() < 1e-3,
        "the sensor published pos x ({px})"
    );
    let entity = entity_of(&app, body_id).unwrap();
    let over = app.world().get::<SignalColorOverride>(entity).unwrap();
    assert!(over.trail.is_some(), "the actuator drove the trail channel");
    assert!(
        over.fill.is_none(),
        "fill left untouched by a tracer actuator"
    );
}

#[test]
fn deleting_a_body_cascades_its_attached_nodes() {
    let mut app = paused_app();
    let body = box_record(Vec2::ZERO, 20.0, 20.0);
    let body_id = body.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: body });
    app.update();
    let node = tracer_record(Vec2::ZERO, NodeAttachment::to(body_id, Vec2::ZERO));
    let node_id = node.id;
    app.world_mut()
        .write_message(SpawnNodeIntent { record: node });
    app.update();
    assert_eq!(node_count(&mut app), 1);

    // Delete the body — the attached node goes too (like a joint cascade).
    app.world_mut().write_message(DeleteIntent {
        targets: vec![body_id],
    });
    app.update();
    assert_eq!(
        node_count(&mut app),
        0,
        "attached node deleted with its body"
    );
    assert!(entity_of(&app, node_id).is_none());

    // Undo restores both the body and its node.
    undo(&mut app);
    assert!(entity_of(&app, body_id).is_some(), "body back");
    assert!(entity_of(&app, node_id).is_some(), "node back");
    assert_eq!(node_count(&mut app), 1);
}

#[test]
fn a_free_tracer_node_does_nothing() {
    use gradiance::render::tracer::TraceTrail;
    let mut app = headless_app();
    // A free tracer (no attachment) — inert, no trail accumulates.
    let free = tracer_record(Vec2::new(50.0, 50.0), NodeAttachment::free());
    let free_id = free.id;
    app.world_mut()
        .write_message(SpawnNodeIntent { record: free });
    step(&mut app, 20);
    let entity = entity_of(&app, free_id).unwrap();
    assert!(
        app.world().get::<TraceTrail>(entity).is_none(),
        "a free tracer accumulates no trail"
    );
}
