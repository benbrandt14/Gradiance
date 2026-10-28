//! Joint sensor sources: a joint's live length and angle are signals —
//! bindable, plottable, and script-readable exactly like a body's speed
//! (`SignalSource::JointLength` / `JointAngle`, read through
//! `physics::queries`).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, step};
use avian2d::prelude::RigidBody;
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;
use gradiance::signal::{SignalBinding, SignalBindings, SignalBus, SignalSink, SignalSource};

/// A static box — the solver leaves it where the test puts it, so the
/// sensor reading is exactly the authored geometry.
fn spawn_static(app: &mut App, pos: Vec2) -> StableId {
    let mut record = box_record(pos, 0.2, 0.2);
    record.physics.rigid_body = RigidBody::Static;
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn spawn_joint(app: &mut App, kind: JointKind, a: StableId, b: Option<StableId>) -> StableId {
    let record = JointRecord {
        id: StableId::new(),
        def: JointDef {
            kind,
            common: JointCommon::default(),
            body_a: a,
            body_b: b,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    };
    let id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    id
}

fn plot(app: &mut App, name: &str, source: SignalSource) {
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: name.into(),
            source,
            map: default(),
            curve: None,
            gradient: default(),
            sink: SignalSink::Plot,
        });
}

fn bus(app: &App, name: &str) -> f32 {
    app.world()
        .resource::<SignalBus>()
        .get(name)
        .expect("published on the bus")
}

/// Teleports a (static) body — test-only direct pose write.
fn set_pose(app: &mut App, id: StableId, pos: Vec2, rot: f32) {
    let entity = entity_of(app, id).unwrap();
    let mut transform = app.world_mut().get_mut::<Transform>(entity).unwrap();
    transform.translation = pos.extend(transform.translation.z);
    transform.rotation = Quat::from_rotation_z(rot);
}

const HINGE: JointKind = JointKind::Hinge {
    limits: None,
    motor: None,
};

#[test]
fn a_hinge_angle_binding_plots_the_relative_rotation() {
    let mut app = headless_app();
    let a = spawn_static(&mut app, Vec2::ZERO);
    let b = spawn_static(&mut app, Vec2::new(0.5, 0.0));
    let joint = spawn_joint(&mut app, HINGE, a, Some(b));
    plot(&mut app, "hinge-angle", SignalSource::JointAngle(joint));
    step(&mut app, 3);
    assert!(bus(&app, "hinge-angle").abs() < 1e-4, "at rest reads 0");

    set_pose(&mut app, b, Vec2::new(0.5, 0.0), 0.4);
    step(&mut app, 3);
    let angle = bus(&app, "hinge-angle");
    assert!((angle - 0.4).abs() < 1e-3, "B rotated 0.4 rad -> {angle}");
}

#[test]
fn a_slider_length_binding_plots_the_anchor_separation() {
    let mut app = headless_app();
    let a = spawn_static(&mut app, Vec2::ZERO);
    let b = spawn_static(&mut app, Vec2::new(0.5, 0.0));
    let slider = JointKind::Slider {
        axis: Vec2::X,
        limits: None,
        motor: None,
    };
    let joint = spawn_joint(&mut app, slider, a, Some(b));
    plot(&mut app, "slide", SignalSource::JointLength(joint));
    step(&mut app, 3);
    assert!((bus(&app, "slide") - 0.5).abs() < 1e-4);

    set_pose(&mut app, b, Vec2::new(1.25, 0.0), 0.0);
    step(&mut app, 3);
    let length = bus(&app, "slide");
    assert!(
        (length - 1.25).abs() < 1e-3,
        "extended to 1.25 m -> {length}"
    );
}

#[test]
fn a_spring_length_reads_against_a_world_pin() {
    // World pin: anchor_b is a world point (here the origin), body A sits
    // 0.75 m away — the length is measured to the world point.
    let mut app = headless_app();
    let a = spawn_static(&mut app, Vec2::new(0.0, 0.75));
    let spring = JointKind::Spring {
        rest_length: 0.75,
        stiffness: 100.0,
        damping: 1.0,
        range: None,
    };
    let joint = spawn_joint(&mut app, spring, a, None);
    plot(&mut app, "spring", SignalSource::JointLength(joint));
    step(&mut app, 3);
    let length = bus(&app, "spring");
    assert!((length - 0.75).abs() < 1e-3, "{length}");
}

#[test]
fn a_script_reads_a_bound_joint_sensor_by_its_binding_name() {
    // No verb needed: `signal-get` resolves any bound source by bus name.
    let mut app = headless_app();
    let a = spawn_static(&mut app, Vec2::ZERO);
    let b = spawn_static(&mut app, Vec2::new(0.5, 0.0));
    let joint = spawn_joint(&mut app, HINGE, a, Some(b));
    plot(&mut app, "hinge-angle", SignalSource::JointAngle(joint));
    set_pose(&mut app, b, Vec2::new(0.5, 0.0), -0.25);
    step(&mut app, 3);
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit("(signal-set \"read-back\" (signal-get \"hinge-angle\"))");
    app.update();
    let read = bus(&app, "read-back");
    assert!((read + 0.25).abs() < 1e-3, "script read {read}");
}
