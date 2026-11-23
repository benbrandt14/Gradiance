//! Gamepad input on the signal bus (brief S-20): a pad block publishes the
//! first gamepad's stick (−1..1) or button (0..1) on the bus while the sim
//! plays, past its dead zone. The value is derived (no command, no undo
//! step); the block itself is scene content.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{headless_app, paused_app, step};
use bevy::input::gamepad::{
    GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
    RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::op::invoke;
use gradiance::prelude::*;
use gradiance::signal::{BlockOp, ComputedSignals, SignalBus};
use serde_json::json;

/// Connects a gamepad the way a driver does: a connection event on a fresh
/// entity, then one frame so bevy inserts the `Gamepad` component.
fn connect_pad(app: &mut App) -> Entity {
    let pad = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(GamepadConnectionEvent::new(
        pad,
        GamepadConnection::Connected {
            name: "Test pad".to_owned(),
            vendor_id: None,
            product_id: None,
        },
    ));
    app.update();
    assert!(
        app.world().get::<Gamepad>(pad).is_some(),
        "the pad connects"
    );
    pad
}

fn move_stick(app: &mut App, pad: Entity, axis: GamepadAxis, value: f32) {
    app.world_mut()
        .write_message(RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(
            pad, axis, value,
        )));
}

fn press(app: &mut App, pad: Entity, button: GamepadButton, value: f32) {
    app.world_mut()
        .write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(
            pad, button, value,
        )));
}

fn bus(app: &App, name: &str) -> Option<f32> {
    app.world().resource::<SignalBus>().get(name)
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

fn play(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(app, 2);
}

/// The brief's done-when: an injected stick event reaches the named signal.
/// No dead zone reads the axis as is; the default 0.1 rescales the rest of
/// the travel; a button reads its analog value. The input records no undo
/// step.
#[test]
fn an_injected_stick_reaches_the_named_signal() {
    let mut app = headless_app();
    let pad = connect_pad(&mut app);
    invoke(
        app.world_mut(),
        "signal.pad",
        json!({"name": "steer", "input": "leftstickx", "dead_zone": 0.0}),
    )
    .expect("a pad input");
    invoke(
        app.world_mut(),
        "signal.pad",
        json!({"name": "steer-dz", "input": "LeftStickX"}),
    )
    .expect("a pad input with the default dead zone");
    invoke(
        app.world_mut(),
        "signal.pad",
        json!({"name": "jump", "input": "South", "dead_zone": 0.0}),
    )
    .expect("a button input");
    step(&mut app, 2);
    play(&mut app);
    let depth = undo_depth(&app);

    move_stick(&mut app, pad, GamepadAxis::LeftStickX, -0.6);
    press(&mut app, pad, GamepadButton::South, 1.0);
    step(&mut app, 2);
    let steer = bus(&app, "steer").unwrap();
    assert!((steer + 0.6).abs() < 1e-3, "the stick reads -0.6 ({steer})");
    let dz = bus(&app, "steer-dz").unwrap();
    assert!(
        (dz + 0.5 / 0.9).abs() < 1e-3,
        "the dead zone rescales the travel ({dz})"
    );
    assert_eq!(bus(&app, "jump"), Some(1.0), "the button is down");

    move_stick(&mut app, pad, GamepadAxis::LeftStickX, 0.05);
    press(&mut app, pad, GamepadButton::South, 0.0);
    step(&mut app, 2);
    assert_eq!(bus(&app, "steer-dz"), Some(0.0), "inside the dead zone: 0");
    assert_eq!(bus(&app, "jump"), Some(0.0), "released: 0");
    assert_eq!(undo_depth(&app), depth, "pad values record no undo step");
}

/// Paused, a pad input reads 0 whatever the stick does; `signal.pad`
/// writes the authored Pad block, and a bad name is an error.
#[test]
fn a_pad_input_reads_zero_while_paused_and_defpad_writes_the_block() {
    let mut app = paused_app();
    let pad = connect_pad(&mut app);
    invoke(
        app.world_mut(),
        "signal.pad",
        json!({"name": "lift", "input": "RightStickY", "dead_zone": 0.25}),
    )
    .expect("a pad input");
    step(&mut app, 2);
    let computed = app.world().resource::<ComputedSignals>();
    let block = computed
        .0
        .iter()
        .find(|s| s.name == "lift")
        .and_then(|s| s.block.clone());
    assert!(
        matches!(
            block.as_ref(),
            Some(BlockOp::Pad { input, dead_zone }) if input == "RightStickY" && (*dead_zone - 0.25).abs() < 1e-6
        ),
        "the authored block: {block:?}"
    );

    move_stick(&mut app, pad, GamepadAxis::RightStickY, 0.9);
    step(&mut app, 2);
    assert_eq!(bus(&app, "lift"), Some(0.0), "paused: 0");

    assert!(
        invoke(
            app.world_mut(),
            "signal.pad",
            json!({"name": "x", "input": "Nope"}),
        )
        .is_err(),
        "an unknown input is an error"
    );
    assert!(
        invoke(
            app.world_mut(),
            "signal.pad",
            json!({"name": "x", "input": "South", "dead_zone": 2.0}),
        )
        .is_err(),
        "a dead zone past 0.9 is an error"
    );
}
