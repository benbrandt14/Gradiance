//! The thruster node (brief N-3): a placeable node that pushes its body
//! along its axis at its own point. The thrust is authored (one undo step per
//! edit, saved with the node); the push is derived; a signal can drive it.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, step, undo};
use avian2d::prelude::{AngularVelocity, ComputedMass, LinearVelocity, Physics};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::command::intent::{DuplicateIntent, PropertyEditIntent, SpawnNodeIntent};
use gradiance::command::property::{PropertyChange, PropertyValue};
use gradiance::core::units::PosRot;
use gradiance::domain::appearance::Appearance;
use gradiance::domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance::domain::thruster::Thruster;
use gradiance::op::invoke;
use gradiance::physics::drive::SignalDrive;
use gradiance::prelude::*;
use gradiance::scene::NodeRecord;
use gradiance::signal::{
    GradientSpec, SignalBinding, SignalBindings, SignalMap, SignalSink, SignalSource,
};
use gradiance::units::{Acceleration2, Angle, Force};
use serde_json::json;

fn zero_gravity(app: &mut App) {
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Acceleration2(Vec2::ZERO);
    app.update();
}

fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// A thruster on `body` at `offset` (body frame), pushing `thrust` newtons
/// along the body-frame `angle`.
fn spawn_thruster(
    app: &mut App,
    body: StableId,
    offset: Vec2,
    angle: f32,
    thrust: f32,
) -> StableId {
    let id = StableId::new();
    let record = NodeRecord {
        id,
        pose: PosRot {
            pos: offset,
            rot: 0.0,
        },
        attachment: NodeAttachment::to(body, offset),
        kind: NodeKind::Thruster(Thruster {
            thrust: Force(thrust),
            angle: Angle(angle),
        }),
        appearance: Appearance::default(),
    };
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    id
}

fn velocity(app: &App, id: StableId) -> Vec2 {
    app.world()
        .get::<LinearVelocity>(entity_of(app, id).unwrap())
        .map_or(Vec2::ZERO, |v| v.0)
}

fn spin(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<AngularVelocity>(entity_of(app, id).unwrap())
        .map_or(0.0, |w| w.0)
}

fn mass(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<ComputedMass>(entity_of(app, id).unwrap())
        .unwrap()
        .value()
}

fn physics_time(app: &App) -> f32 {
    app.world().resource::<Time<Physics>>().elapsed_secs()
}

/// The measured acceleration of `body` over `frames` frames: the change of
/// its velocity over the physics time that passed.
fn acceleration(app: &mut App, body: StableId, frames: usize) -> Vec2 {
    let (v0, t0) = (velocity(app, body), physics_time(app));
    step(app, frames);
    let (v1, t1) = (velocity(app, body), physics_time(app));
    assert!(t1 > t0, "the simulation ran");
    (v1 - v0) / (t1 - t0)
}

fn kind_of(app: &App, node: StableId) -> NodeKind {
    app.world()
        .get::<NodeKind>(entity_of(app, node).unwrap())
        .unwrap()
        .clone()
}

fn bind_thrust(app: &mut App, name: &str, source: &str, node: StableId) {
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: name.into(),
            source: SignalSource::Named(source.into()),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::Thrust(node),
        });
}

/// The brief's behaviour test: with gravity off, a thruster at the centre of
/// a free body accelerates it at F/m along the node's axis, and does not
/// turn it.
#[test]
fn a_thruster_accelerates_a_free_body_at_f_over_m_along_its_axis() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 2.0, 1.0));
    // 30 degrees above the body's +x.
    let angle = 30_f32.to_radians();
    let thrust = 3.0;
    spawn_thruster(&mut app, body, Vec2::ZERO, angle, thrust);
    step(&mut app, 2);

    let m = mass(&app, body);
    assert!(m > 0.0, "a dynamic body has mass");
    let a = acceleration(&mut app, body, 60);
    let expected = Vec2::from_angle(angle) * thrust / m;
    assert!(
        (a - expected).length() < 0.02 * expected.length(),
        "a = {a}, F/m along the axis = {expected}"
    );
    assert!(
        spin(&app, body).abs() < 1e-3,
        "a push through the centre does not turn the body ({})",
        spin(&app, body)
    );
}

/// Off the centre the push also turns the body, and the axis turns with it.
#[test]
fn an_off_centre_thruster_turns_its_body() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 2.0, 0.5));
    // At the right end, pushing up: counter-clockwise.
    spawn_thruster(
        &mut app,
        body,
        Vec2::new(1.0, 0.0),
        std::f32::consts::FRAC_PI_2,
        2.0,
    );
    step(&mut app, 30);
    assert!(spin(&app, body) > 0.1, "it turns ({})", spin(&app, body));
}

/// A free thruster and one on a static body push nothing.
#[test]
fn a_thruster_without_a_dynamic_body_pushes_nothing() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let mut ground = box_record(Vec2::ZERO, 2.0, 0.5);
    ground.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    let ground = spawn_body(&mut app, ground);
    spawn_thruster(&mut app, ground, Vec2::ZERO, 0.0, 50.0);
    let record = NodeRecord {
        id: StableId::new(),
        pose: PosRot {
            pos: Vec2::new(5.0, 0.0),
            rot: 0.0,
        },
        attachment: NodeAttachment::free(),
        kind: NodeKind::Thruster(Thruster::default()),
        appearance: Appearance::default(),
    };
    app.world_mut().write_message(SpawnNodeIntent { record });
    step(&mut app, 30);
    assert_eq!(velocity(&app, ground), Vec2::ZERO);
}

/// A thrust edit is one undo step, and the push follows the authored value.
#[test]
fn a_thrust_edit_is_one_undo_step() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
    let node = spawn_thruster(&mut app, body, Vec2::ZERO, 0.0, 1.0);
    let before = kind_of(&app, node);
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    let NodeKind::Thruster(mut edited) = before.clone() else {
        panic!("a thruster");
    };
    edited.thrust = Force(4.0);
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: node,
            old: PropertyValue::NodeKind(before.clone()),
            new: PropertyValue::NodeKind(NodeKind::Thruster(edited)),
        }],
    });
    step(&mut app, 2);
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        depth + 1,
        "one edit, one step"
    );
    let m = mass(&app, body);
    let a = acceleration(&mut app, body, 30);
    assert!(
        (a.x - 4.0 / m).abs() < 0.05 * 4.0 / m,
        "the edited thrust ({a})"
    );

    // In play, the first undo rewinds the run ("simulate"); the next one
    // takes the edit back.
    undo(&mut app);
    assert_eq!(
        app.world().resource::<HistoryInfo>().redo_label,
        Some("simulate")
    );
    undo(&mut app);
    assert_eq!(kind_of(&app, node), before, "undo restores the thrust");
    assert!(
        app.world()
            .get::<gradiance::domain::tracer::Tracer>(entity_of(&app, node).unwrap())
            .is_none(),
        "a thruster carries no tracer component"
    );
}

/// N-1 drives the thrust: a bound signal replaces the authored thrust (the
/// authored node never changes, no undo step), and unbinding brings the
/// authored thrust back.
#[test]
fn a_signal_drives_the_thrust_until_unbound() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
    let node = spawn_thruster(&mut app, body, Vec2::ZERO, 0.0, 1.0);
    let authored = kind_of(&app, node);
    app.world_mut()
        .resource_mut::<gradiance::signal::Graph>()
        .edit(&gradiance::graph::GraphEdit::Param {
            id: StableId::new(),
            name: "burn".into(),
            value: -5.0,
            min: -10.0,
            max: 10.0,
        })
        .unwrap();
    bind_thrust(&mut app, "throttle", "burn", node);
    step(&mut app, 3);
    assert_eq!(
        app.world()
            .get::<SignalDrive>(entity_of(&app, node).unwrap())
            .and_then(|d| d.thrust),
        Some(-5.0),
        "the binding drives the node"
    );
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    let m = mass(&app, body);
    let a = acceleration(&mut app, body, 30);
    assert!(
        (a.x + 5.0 / m).abs() < 0.05 * 5.0 / m,
        "the driven thrust pushes backwards ({a})"
    );
    assert_eq!(kind_of(&app, node), authored, "the authored thrust stays");
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        depth,
        "a drive records no undo step"
    );

    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .retain(|b| b.name != "throttle");
    step(&mut app, 2);
    let a = acceleration(&mut app, body, 30);
    assert!(
        (a.x - 1.0 / m).abs() < 0.05 / m,
        "the authored thrust is back ({a})"
    );
}

/// `(thruster body x y angle thrust)` places what the tool places: attached
/// at the world point, the world angle stored in the body's frame, one undo
/// step.
#[test]
fn the_thruster_operation_attaches_in_the_body_frame() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let mut record = box_record(Vec2::new(1.0, 2.0), 2.0, 1.0);
    record.pose.rot = std::f32::consts::FRAC_PI_2;
    let body = spawn_body(&mut app, record);
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    let handle = invoke(
        app.world_mut(),
        "node.thruster",
        json!({"body": body.0.to_string(), "x": 1.0, "y": 2.5, "angle": 0.0, "thrust": 7.0}),
    )
    .expect("the operation lowers");
    app.update();
    let id = handle
        .and_then(|v| v.as_str().map(str::to_owned))
        .expect("a handle");
    let (attachment, kind) = app
        .world_mut()
        .query_filtered::<(&StableId, &NodeAttachment, &NodeKind), With<BehaviorNode>>()
        .iter(app.world())
        .find(|(n, ..)| n.0.to_string() == id)
        .map(|(_, a, k)| (*a, k.clone()))
        .expect("the node exists");
    assert_eq!(attachment.target, Some(body));
    // The body is turned a quarter: the world +y half-metre is its local +x.
    assert!((attachment.offset.value() - Vec2::new(0.5, 0.0)).length() < 1e-5);
    let NodeKind::Thruster(thruster) = kind else {
        panic!("a thruster");
    };
    assert!((thruster.angle.0 + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert!((thruster.thrust.0 - 7.0).abs() < 1e-6);
    assert_eq!(app.world().resource::<HistoryInfo>().undo_depth, depth + 1);
}

/// Duplicating a body copies its thruster and the binding that drives it,
/// onto the copy's thruster.
#[test]
fn duplicating_a_body_copies_its_thruster_and_its_drive() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
    let node = spawn_thruster(&mut app, body, Vec2::ZERO, 0.0, 1.0);
    bind_thrust(&mut app, "throttle", "burn", node);
    app.world_mut().write_message(DuplicateIntent {
        sources: vec![body],
        offset: Vec2::new(3.0, 0.0),
    });
    step(&mut app, 2);
    let thrusters: Vec<StableId> = app
        .world_mut()
        .query_filtered::<(&StableId, &NodeKind), With<BehaviorNode>>()
        .iter(app.world())
        .filter(|(_, k)| matches!(k, NodeKind::Thruster(_)))
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(thrusters.len(), 2, "the copy has its own thruster");
    let copy = thrusters.into_iter().find(|id| *id != node).unwrap();
    let bindings = &app.world().resource::<SignalBindings>().0;
    assert!(
        bindings.iter().any(|b| b.sink == SignalSink::Thrust(copy)),
        "the copy's thruster is driven too: {bindings:?}"
    );
}
