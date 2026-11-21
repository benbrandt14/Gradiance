//! `signal.bind` / `signal.unbind` (brief Q-2): a script makes and removes
//! a signal binding, as a wire on the node canvas or a Signals row does.
//! Each call is one `settings` undo step; a bad sink, a target that names
//! nothing and an unknown name are typed errors that record nothing.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, step, undo};
use avian2d::prelude::{AngularVelocity, RevoluteJoint};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::prelude::*;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::signal::{SignalBindings, SignalBus, SignalSink, SignalSource};
use gradiance::units::{Acceleration2, Angle, AngularVelocity as Omega, Displacement};

/// The wheel's authored motor speed (rad/s).
const AUTHORED_SPEED: f32 = 1.0;

/// A 0.6 m wheel pinned to the world at its centre by a hinge with an
/// enabled motor at [`AUTHORED_SPEED`], in zero gravity. Returns (wheel,
/// joint); both are index 0.
fn motor_wheel(app: &mut App) -> (StableId, StableId) {
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Acceleration2(Vec2::ZERO);
    let mut wheel = box_record(Vec2::ZERO, 0.6, 0.6);
    wheel.shape = ShapeDef::Circle { radius: 0.3 };
    let wheel_id = wheel.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: wheel });
    app.update();
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

/// Submits Lisp `source`, advances two frames (a settings edit records on
/// the frame after it settles) and returns whether the run succeeded.
fn run(app: &mut App, source: &str) -> Result<(), String> {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    step(app, 2);
    let log = app.world().resource::<ScriptLog>();
    let last = log.0.last().expect("the run is logged");
    if last.ok {
        Ok(())
    } else {
        Err(last.output.clone())
    }
}

fn depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

fn binding_names(app: &App) -> Vec<String> {
    app.world()
        .resource::<SignalBindings>()
        .0
        .iter()
        .map(|b| b.name.clone())
        .collect()
}

fn spin(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<AngularVelocity>(entity_of(app, id).unwrap())
        .map_or(0.0, |w| w.0)
}

fn motor_target(app: &App, joint: StableId) -> f32 {
    app.world()
        .get::<RevoluteJoint>(entity_of(app, joint).unwrap())
        .expect("the derived engine joint")
        .motor
        .target_velocity
}

/// `bind` drives the motor from a `defsignal`, `unbind` gives the authored
/// motor back, and each is one undo step that undo reverts.
#[test]
fn bind_drives_a_motor_and_unbind_releases_it() {
    let mut app = headless_app();
    let (wheel, joint) = motor_wheel(&mut app);
    run(&mut app, "(defsignal \"osc\" \"3\")").unwrap();
    let before = depth(&app);

    run(&mut app, "(bind \"spin\" \"osc\" \"motor-speed\" 0)").unwrap();
    assert_eq!(depth(&app), before + 1, "bind is one undo step");
    let binding = app.world().resource::<SignalBindings>().0[0].clone();
    assert_eq!(binding.name, "spin");
    assert_eq!(binding.source, SignalSource::Named("osc".into()));
    assert_eq!(binding.sink, SignalSink::MotorSpeed(joint));
    step(&mut app, 60);
    let w = spin(&app, wheel);
    assert!((w - 3.0).abs() < 0.2, "the wheel follows the signal ({w})");

    run(&mut app, "(unbind \"spin\")").unwrap();
    assert_eq!(depth(&app), before + 2, "unbind is one undo step");
    assert!(binding_names(&app).is_empty(), "the binding is gone");
    step(&mut app, 2);
    assert!(
        (motor_target(&app, joint) - AUTHORED_SPEED).abs() < 1e-6,
        "the authored motor speed is back"
    );

    undo(&mut app);
    step(&mut app, 2);
    assert_eq!(binding_names(&app), ["spin"], "undo restores the binding");
    undo(&mut app);
    step(&mut app, 2);
    assert!(binding_names(&app).is_empty(), "undo removes the binding");
}

/// Re-binding a name replaces the binding; a quantity source with `of`
/// publishes the body's read on the bus, and `min`/`max` set the map.
#[test]
fn bind_reads_a_quantity_and_replaces_by_name() {
    let mut app = headless_app();
    let (wheel, _) = motor_wheel(&mut app);
    run(&mut app, "(bind \"w\" \"t\" \"fill\" 0)").unwrap();
    run(&mut app, "(bind \"w\" \"spin\" \"plot\" -1 (list 0) 0 2)").unwrap();
    let bindings = app.world().resource::<SignalBindings>().0.clone();
    assert_eq!(bindings.len(), 1, "re-binding replaces");
    assert_eq!(bindings[0].source, SignalSource::Spin(wheel));
    assert_eq!(bindings[0].sink, SignalSink::Plot);
    assert!((bindings[0].map.in_max - 2.0).abs() < 1e-6);
    step(&mut app, 60);
    let published = app.world().resource::<SignalBus>().get("w").unwrap();
    assert!(
        (published - AUTHORED_SPEED).abs() < 0.1,
        "the bus carries the wheel's spin ({published})"
    );
}

/// Every refusal is a script error with no step and no change.
#[test]
fn bad_binds_are_typed_errors() {
    let mut app = headless_app();
    motor_wheel(&mut app);
    let before = depth(&app);
    for (source, needle) in [
        ("(bind \"x\" \"t\" \"motor\" 0)", "unknown sink"),
        ("(bind \"x\" \"t\" \"motor-speed\" 5)", "names no joint"),
        ("(bind \"x\" \"t\" \"thrust\" 0)", "names no node"),
        ("(bind \"x\" \"t\" \"motor-speed\")", "needs a target"),
        (
            "(bind \"x\" \"distance\" \"plot\" 0 (list 0))",
            "two bodies",
        ),
        (
            "(bind \"x\" \"nonsense\" \"plot\" 0 (list 0))",
            "unknown quantity",
        ),
        ("(unbind \"nope\")", "no binding"),
    ] {
        let err = run(&mut app, source).expect_err(source);
        assert!(err.contains(needle), "{source}: {err}");
    }
    assert_eq!(depth(&app), before, "no refusal records a step");
    assert!(binding_names(&app).is_empty());
}
