//! Keys as node-graph inputs (brief N-2): a key block publishes `1` (or
//! `+1`/`-1` for a pair) on the bus while its key is held in play; the
//! value is derived (no command, no undo step), the block itself is scene
//! content. While the sim plays a graph-bound key is the graph's, not the
//! editor's; paused, the editor keeps every key; a field with the keyboard
//! keeps it from both.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, paused_app, step, undo};
use avian2d::prelude::{AngularVelocity, RevoluteJoint};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::interaction::KeyboardCaptured;
use gradiance::interaction::graph_keys::GraphKeys;
use gradiance::op::invoke;
use gradiance::prelude::*;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::signal::{
    BlockOp, ComputedSignals, GradientSpec, SignalBinding, SignalBindings, SignalBus, SignalMap,
    SignalSink, SignalSource,
};
use gradiance::units::{Angle, AngularVelocity as Omega, Displacement};
use serde_json::json;

fn zero_gravity(app: &mut App) {
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = gradiance::units::Acceleration2(Vec2::ZERO);
    app.update();
}

/// A 0.6 m wheel pinned to the world by a hinge whose motor is enabled at
/// 0 rad/s: it holds still until a signal drives it. Returns (wheel, joint).
fn still_wheel(app: &mut App) -> (StableId, StableId) {
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
                    target_velocity: Omega(0.0),
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

/// Submits Lisp `source` and advances one frame; panics on a script error.
fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
    let log = app.world().resource::<ScriptLog>();
    let last = log.0.last().map(|e| (e.ok, e.output.clone()));
    assert!(
        last.as_ref().is_some_and(|(ok, _)| *ok),
        "the script ran: {last:?}"
    );
}

fn bind_motor(app: &mut App, source: &str, joint: StableId) {
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "drive".into(),
            source: SignalSource::Named(source.into()),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::MotorSpeed(joint),
        });
}

fn bus(app: &App, name: &str) -> Option<f32> {
    app.world().resource::<SignalBus>().get(name)
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

fn authored_def(app: &App, joint: StableId) -> JointDef {
    app.world()
        .get::<JointDef>(entity_of(app, joint).unwrap())
        .unwrap()
        .clone()
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

fn tool(app: &App) -> ToolState {
    *app.world().resource::<State<ToolState>>().get()
}

/// The brief's behaviour test: a key-driven motor, run from a script that
/// declares the key input and holds its keys through `ui.key`. `E` turns the
/// wheel forward at 4 rad/s, `Q` back; released, the wheel stops. The
/// authored motor never changes and the held keys record no undo step.
#[test]
fn a_script_holds_keys_that_drive_a_motor_through_a_key_input() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let (wheel, joint) = still_wheel(&mut app);
    let authored = authored_def(&app, joint);
    run(
        &mut app,
        r#"(begin (defkey "throttle" "E" "Q") (defsignal "spin" "throttle 4 *"))"#,
    );
    bind_motor(&mut app, "spin", joint);
    step(&mut app, 5);
    assert_eq!(bus(&app, "throttle"), Some(0.0), "no key held: 0");
    assert!(spin(&app, wheel).abs() < 1e-3, "the wheel holds still");
    let depth = undo_depth(&app);

    // Hold E for 60 frames: the input reads +1 and the wheel spins up to the
    // driven 4 rad/s.
    run(&mut app, r#"(ui-key "E" 60)"#);
    step(&mut app, 40);
    assert_eq!(bus(&app, "key@E"), Some(1.0), "E is held");
    assert_eq!(bus(&app, "throttle"), Some(1.0));
    assert!((motor_target(&app, joint) - 4.0).abs() < 1e-4);
    let w = spin(&app, wheel);
    assert!((w - 4.0).abs() < 0.5, "E turns the wheel forward ({w})");

    // Released: the input reads 0 and the motor holds the wheel still.
    step(&mut app, 60);
    assert_eq!(bus(&app, "throttle"), Some(0.0), "released: 0");
    assert!(motor_target(&app, joint).abs() < 1e-6);
    assert!(spin(&app, wheel).abs() < 0.5, "the wheel stops");

    // Q is the pair's other key: -1, the wheel turns back.
    run(&mut app, r#"(ui-key "Q" 60)"#);
    step(&mut app, 40);
    assert_eq!(bus(&app, "throttle"), Some(-1.0), "Q is held");
    let w = spin(&app, wheel);
    assert!((w + 4.0).abs() < 0.5, "Q turns the wheel back ({w})");
    step(&mut app, 30);

    assert_eq!(
        authored_def(&app, joint),
        authored,
        "the authored motor never changes"
    );
    assert_eq!(undo_depth(&app), depth, "held keys record no undo step");
}

/// Paused, a graph-bound key is the editor's (`W` picks the Weld tool) and
/// the input reads 0; in play the same key is the input's and the tool stays.
#[test]
fn a_bound_key_is_the_editors_while_paused_and_the_inputs_in_play() {
    let mut app = paused_app();
    invoke(
        app.world_mut(),
        "signal.key",
        json!({"name": "w-key", "key": "w"}),
    )
    .expect("a key input");
    step(&mut app, 2);
    assert!(
        app.world()
            .resource::<GraphKeys>()
            .bound()
            .contains_code(KeyCode::KeyW)
    );
    assert_eq!(tool(&app), ToolState::Select);

    invoke(app.world_mut(), "ui.key", json!({"key": "W", "hold": 5})).unwrap();
    step(&mut app, 4);
    assert_eq!(tool(&app), ToolState::Weld, "paused: W picks the Weld tool");
    assert_eq!(bus(&app, "w-key"), Some(0.0), "paused: the input reads 0");
    step(&mut app, 4);

    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Select);
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 2);
    invoke(app.world_mut(), "ui.key", json!({"key": "W", "hold": 5})).unwrap();
    step(&mut app, 4);
    assert_eq!(bus(&app, "w-key"), Some(1.0), "in play: the input reads 1");
    assert_eq!(tool(&app), ToolState::Select, "in play: W is the input's");
    step(&mut app, 4);
    assert_eq!(bus(&app, "w-key"), Some(0.0));

    // A key no input binds still reaches the editor in play.
    invoke(app.world_mut(), "ui.key", json!({"key": "B"})).unwrap();
    step(&mut app, 3);
    assert_eq!(tool(&app), ToolState::Box);
}

/// A field with the keyboard keeps a held key from the input (and, as
/// before, from the editor).
#[test]
fn a_field_with_the_keyboard_keeps_the_key_from_the_input() {
    let mut app = headless_app();
    invoke(
        app.world_mut(),
        "signal.key",
        json!({"name": "throttle", "key": "E"}),
    )
    .unwrap();
    step(&mut app, 2);
    app.world_mut().insert_resource(KeyboardCaptured(true));
    invoke(app.world_mut(), "ui.key", json!({"key": "E", "hold": 10})).unwrap();
    step(&mut app, 5);
    assert_eq!(bus(&app, "throttle"), Some(0.0), "typing is not driving");
    app.world_mut().insert_resource(KeyboardCaptured(false));
    step(&mut app, 2);
    assert_eq!(
        bus(&app, "throttle"),
        Some(1.0),
        "the key still held drives"
    );
    step(&mut app, 10);
}

/// The key input is scene content: declaring it is one `settings` undo
/// step, undo removes it, and it is the `Key` block the graph shows.
#[test]
fn a_key_input_is_scene_content_with_one_undo_step() {
    let mut app = paused_app();
    // A first command, so history has a baseline to step back to.
    app.world_mut().write_message(SpawnBodyIntent {
        record: box_record(Vec2::ZERO, 0.4, 0.4),
    });
    step(&mut app, 2);
    let depth = undo_depth(&app);
    invoke(
        app.world_mut(),
        "signal.key",
        json!({"name": "steer", "key": "arrowright", "negative": "Left"}),
    )
    .unwrap();
    step(&mut app, 3);
    let block = app
        .world()
        .resource::<ComputedSignals>()
        .0
        .iter()
        .find(|c| c.name == "steer")
        .and_then(|c| c.block.clone());
    assert_eq!(
        block,
        Some(BlockOp::Key {
            key: "Right".into(),
            negative: Some("Left".into()),
        }),
        "key names are saved canonical"
    );
    assert_eq!(undo_depth(&app), depth + 1, "one settings step");
    undo(&mut app);
    step(&mut app, 2);
    assert!(
        app.world().resource::<GraphKeys>().bound().is_empty(),
        "undo removes the input and its keys"
    );

    let bad = invoke(
        app.world_mut(),
        "signal.key",
        json!({"name": "x", "key": "Hyper"}),
    );
    assert!(bad.is_err(), "an unknown key name is refused");
}
