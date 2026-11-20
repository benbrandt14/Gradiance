//! Signals that drive physics (brief N-1): a binding whose sink is a motor
//! speed, a motor cap, a field strength or a gravity scale writes the derived
//! `SignalDrive`, which the physics layer prefers over the authored value.
//! Unbinding restores the authored value with no command; the authored
//! components never change and the drive records no undo step.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, step};
use avian2d::prelude::{AngularVelocity, LinearVelocity, RevoluteJoint, RigidBody};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::field::FieldSource;
use gradiance::op::invoke;
use gradiance::physics::drive::SignalDrive;
use gradiance::prelude::*;
use gradiance::signal::{
    GradientSpec, SignalBinding, SignalBindings, SignalBus, SignalMap, SignalSink, SignalSource,
};
use gradiance::units::{Acceleration2, Angle, AngularVelocity as Omega, Displacement};
use serde_json::json;

/// The wheel's authored motor speed (rad/s).
const AUTHORED_SPEED: f32 = 1.0;

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

/// A 0.6 m wheel pinned to the world at its centre by a hinge with an
/// enabled motor at [`AUTHORED_SPEED`] (auto cap). Returns (wheel, joint).
fn motor_wheel(app: &mut App) -> (StableId, StableId) {
    let mut wheel = box_record(Vec2::ZERO, 0.6, 0.6);
    wheel.shape = ShapeDef::Circle { radius: 0.3 };
    let wheel_id = spawn_body(app, wheel);
    let record = JointRecord {
        id: StableId::new(),
        def: JointDef {
            kind: JointKind::Hinge {
                limits: None,
                motor: Some(AngularMotorDef {
                    target_velocity: Omega(AUTHORED_SPEED),
                    ..default()
                }),
            },
            common: JointCommon::default(),
            body_a: wheel_id,
            body_b: None,
            anchor_a: Displacement(Vec2::ZERO),
            anchor_b: Displacement(Vec2::ZERO),
            rest_rot_a: Angle(0.0),
            rest_rot_b: Angle(0.0),
        },
    };
    let joint_id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    (wheel_id, joint_id)
}

/// An actuator binding of the named bus signal `source` into `sink`.
fn bind(app: &mut App, name: &str, source: &str, sink: SignalSink) {
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: name.into(),
            source: SignalSource::Named(source.into()),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink,
        });
}

fn unbind(app: &mut App, name: &str) {
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .retain(|b| b.name != name);
}

/// A constant knob on the bus (a `defparam`).
fn param(app: &mut App, name: &str, value: f32) {
    app.world_mut()
        .resource_mut::<gradiance::signal::Graph>()
        .edit(&gradiance::graph::GraphEdit::Param {
            id: StableId::new(),
            name: name.into(),
            value,
            min: -10.0,
            max: 10.0,
        })
        .unwrap();
}

fn spin(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<AngularVelocity>(entity_of(app, id).unwrap())
        .map_or(0.0, |w| w.0)
}

fn velocity(app: &App, id: StableId) -> Vec2 {
    app.world()
        .get::<LinearVelocity>(entity_of(app, id).unwrap())
        .map_or(Vec2::ZERO, |v| v.0)
}

fn motor_target(app: &App, joint: StableId) -> f32 {
    app.world()
        .get::<RevoluteJoint>(entity_of(app, joint).unwrap())
        .expect("the derived engine joint")
        .motor
        .target_velocity
}

fn drive_of(app: &App, id: StableId) -> Option<SignalDrive> {
    app.world()
        .get::<SignalDrive>(entity_of(app, id).unwrap())
        .copied()
}

fn authored_def(app: &App, joint: StableId) -> JointDef {
    app.world()
        .get::<JointDef>(entity_of(app, joint).unwrap())
        .unwrap()
        .clone()
}

/// The brief's behaviour test: `4 sin(π t)` bound to the hinge's motor speed
/// turns the wheel one way for a second and back the next, every half period.
/// Unbinding drops the drive and the authored speed comes back.
#[test]
fn a_sine_signal_on_a_hinge_motor_reverses_the_wheel_each_half_period() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let (wheel, joint) = motor_wheel(&mut app);
    let authored = authored_def(&app, joint);

    // The defsignal: period 2 s, so the drive changes sign every second.
    invoke(
        app.world_mut(),
        "signal.computed",
        json!({"name": "osc", "expr": "t 3.14159265 * sin 4 *"}),
    )
    .expect("defsignal lowers");
    app.update();
    bind(&mut app, "drive", "osc", SignalSink::MotorSpeed(joint));
    step(&mut app, 5);
    assert!(
        drive_of(&app, joint).is_some_and(|d| d.motor_speed.is_some()),
        "the binding drives the joint"
    );
    let depth = app.world().resource::<HistoryInfo>().undo_depth;

    // Sample the wheel at the middle of each half period (where |sin| = 1):
    // it turns the way the signal says, at about the signal's speed.
    let mut checked = 0;
    let mut last_half = None;
    for _ in 0..300 {
        step(&mut app, 1);
        let t = app.world().resource::<SignalBus>().get("t").unwrap();
        let phase = t.fract();
        let half = t.floor();
        if (0.45..0.55).contains(&phase) && last_half != Some(half) {
            last_half = Some(half);
            let expected = 4.0 * (std::f32::consts::PI * t).sin();
            let w = spin(&app, wheel);
            assert!(
                w.signum() == expected.signum() && (w - expected).abs() < 1.0,
                "at t = {t:.2} the wheel turns at {w:.2} rad/s, the signal says {expected:.2}"
            );
            let target = motor_target(&app, joint);
            assert!(
                (target - app.world().resource::<SignalBus>().get("osc").unwrap()).abs() < 1e-4,
                "the live motor target is the signal ({target})"
            );
            checked += 1;
        }
    }
    assert!(checked >= 4, "saw four half periods ({checked})");
    assert_eq!(
        authored_def(&app, joint),
        authored,
        "the authored motor never changes"
    );
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        depth,
        "a drive records no undo step"
    );

    // Unbind: the drive goes, the authored speed returns, no command needed.
    unbind(&mut app, "drive");
    step(&mut app, 2);
    assert!(
        drive_of(&app, joint).is_none(),
        "unbinding removes the drive"
    );
    assert!(
        (motor_target(&app, joint) - AUTHORED_SPEED).abs() < 1e-6,
        "the authored speed is back in the engine joint"
    );
    step(&mut app, 90);
    let w = spin(&app, wheel);
    assert!(
        (w - AUTHORED_SPEED).abs() < 0.1,
        "the wheel settles at the authored speed ({w})"
    );
}

/// A zero cap stops the motor pushing: the wheel stays still although its
/// authored motor would turn it. Unbinding brings the authored auto cap back.
#[test]
fn a_motor_cap_drive_limits_the_motor() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let (wheel, joint) = motor_wheel(&mut app);
    // The wheel spins up under the authored motor before the cap binds; stop
    // it so the test starts from rest.
    param(&mut app, "cap", 0.0);
    bind(&mut app, "limit", "cap", SignalSink::MotorCap(joint));
    step(&mut app, 2);
    let entity = entity_of(&app, wheel).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .insert(AngularVelocity(0.0));
    step(&mut app, 60);
    assert!(
        spin(&app, wheel).abs() < 1e-3,
        "a zero cap leaves the wheel at rest ({})",
        spin(&app, wheel)
    );
    unbind(&mut app, "limit");
    step(&mut app, 60);
    assert!(
        (spin(&app, wheel) - AUTHORED_SPEED).abs() < 0.1,
        "the auto cap is back ({})",
        spin(&app, wheel)
    );
}

/// A gravity-scale drive of 0 floats a body under full gravity; its authored
/// `GravityScale` stays 1, and unbinding lets it fall again.
#[test]
fn a_gravity_scale_drive_floats_a_body_until_unbound() {
    let mut app = headless_app();
    app.update();
    let ball = spawn_body(&mut app, box_record(Vec2::new(0.0, 5.0), 0.4, 0.4));
    param(&mut app, "float", 0.0);
    bind(&mut app, "lift", "float", SignalSink::GravityScale(ball));
    step(&mut app, 2);
    let entity = entity_of(&app, ball).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .insert(LinearVelocity(Vec2::ZERO));
    step(&mut app, 60);
    let v = velocity(&app, ball);
    assert!(v.length() < 1e-3, "a zero scale floats the body ({v})");
    let authored = app
        .world()
        .get::<avian2d::prelude::GravityScale>(entity_of(&app, ball).unwrap())
        .unwrap()
        .0;
    assert!(
        (authored - 1.0).abs() < 1e-6,
        "authored scale untouched ({authored})"
    );

    unbind(&mut app, "lift");
    step(&mut app, 30);
    assert!(
        velocity(&app, ball).y < -1.0,
        "unbound, it falls ({})",
        velocity(&app, ball)
    );
    assert!(
        app.world()
            .get::<avian2d::prelude::ConstantLinearAcceleration>(entity_of(&app, ball).unwrap())
            .is_none(),
        "the derived correction goes with the drive"
    );
}

/// A field-strength drive of 0 switches an attractor off; unbinding switches
/// its authored pull back on.
#[test]
fn a_field_strength_drive_replaces_the_authored_strength() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let mut sun = box_record(Vec2::ZERO, 1.0, 1.0);
    sun.physics.rigid_body = RigidBody::Static;
    sun.field = Some(FieldSource::default());
    let sun = spawn_body(&mut app, sun);
    let probe = spawn_body(&mut app, box_record(Vec2::new(3.0, 0.0), 0.2, 0.2));
    param(&mut app, "off", 0.0);
    bind(&mut app, "dim", "off", SignalSink::FieldStrength(sun));
    step(&mut app, 2);
    let entity = entity_of(&app, probe).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .insert(LinearVelocity(Vec2::ZERO));
    step(&mut app, 30);
    let v = velocity(&app, probe);
    assert!(v.length() < 1e-3, "a zero strength pulls nothing ({v})");

    unbind(&mut app, "dim");
    step(&mut app, 10);
    assert!(
        velocity(&app, probe).x < -0.1,
        "the authored attraction is back ({})",
        velocity(&app, probe)
    );
}
