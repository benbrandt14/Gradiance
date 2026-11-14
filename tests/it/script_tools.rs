//! Script tools (brief 3-5): a `.scm` registers a click-collecting tool, the
//! Rust-side draft collects the points through the ordinary tool driver, and
//! the script body runs **once**, on the last point.

#![allow(clippy::expect_used)]

use crate::harness::{body_count, paused_app, undo};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use gradiance::core::script_tools::ScriptTools;
use gradiance::domain::Body;
use gradiance::domain::shape::ShapeDef;
use gradiance::interaction::tools::script_tool::ScriptTool;
use gradiance::prelude::*;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};

/// A two-click ramp: a thin box from the first point to the second.
const RAMP: &str = r#"(register-tool "ramp" 2
  "(let* ((ax (tool-x 0)) (ay (tool-y 0))
          (bx (tool-x 1)) (by (tool-y 1))
          (dx (- bx ax)) (dy (- by ay)))
     (spawn-box-rot (/ (+ ax bx) 2) (/ (+ ay by) 2)
                    (sqrt (+ (* dx dx) (* dy dy))) 0.1
                    (angle (make-rectangular dx dy))))")"#;

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn button(app: &mut App, button: MouseButton, down: bool) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    if down {
        input.press(button);
    } else {
        input.release(button);
    }
}

/// Sends an Escape key event. A key pressed straight on `ButtonInput` loses
/// its `just_pressed` to the input plugin's per-frame clear, so the event
/// path is the one a real key takes.
fn escape(app: &mut App, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// A full left click at `p` (press frame + release frame).
fn click(app: &mut App, p: Vec2) {
    set_cursor(app, p);
    button(app, MouseButton::Left, true);
    app.update();
    button(app, MouseButton::Left, false);
    app.update();
}

/// How many script runs the VM has made (every run logs one entry).
fn runs(app: &App) -> usize {
    app.world().resource::<ScriptLog>().0.len()
}

/// Registers the ramp and selects it; returns the run count after
/// registering.
fn ramp_app() -> App {
    let mut app = paused_app();
    app.world_mut().resource_mut::<ScriptInputs>().submit(RAMP);
    app.update();
    let index = app
        .world()
        .resource::<ScriptTools>()
        .index_of("ramp")
        .expect("register-tool added the ramp");
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Script(index));
    app.update();
    app
}

#[test]
fn a_two_point_ramp_spawns_one_rotated_box_and_one_undo_removes_it() {
    let mut app = ramp_app();
    let before = runs(&app);
    assert_eq!(before, 1, "only the registration has run");

    click(&mut app, Vec2::ZERO);
    assert_eq!(app.world().resource::<ScriptTool>().points(), &[Vec2::ZERO]);
    assert_eq!(runs(&app), before, "the VM is not entered mid-gesture");
    assert_eq!(body_count(&mut app), 0);

    click(&mut app, Vec2::new(2.0, 1.0));
    app.update();
    assert_eq!(runs(&app), before + 1, "one run per gesture");
    let ok = app.world().resource::<ScriptLog>().0.last().map(|e| e.ok);
    assert_eq!(
        ok,
        Some(true),
        "{:?}",
        app.world().resource::<ScriptLog>().0
    );
    assert_eq!(body_count(&mut app), 1);

    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    let (shape, transform) = q.iter(app.world()).next().expect("the ramp");
    let ShapeDef::Box { width, height } = shape.clone() else {
        panic!("expected a box, got {shape:?}");
    };
    assert!(
        (width - 5.0_f32.sqrt()).abs() < 1e-4,
        "spans the segment ({width})"
    );
    assert!((height - 0.1).abs() < 1e-6);
    assert!(
        transform
            .translation
            .truncate()
            .distance(Vec2::new(1.0, 0.5))
            < 1e-4,
        "centred on the segment ({:?})",
        transform.translation
    );
    let rot = transform.rotation.to_euler(EulerRot::ZYX).0;
    assert!(
        (rot - 0.5_f32.atan()).abs() < 1e-4,
        "along the segment ({rot})"
    );

    undo(&mut app);
    assert_eq!(body_count(&mut app), 0, "one undo removes the ramp");
}

#[test]
fn cancelling_mid_gesture_spawns_nothing() {
    let mut app = ramp_app();
    let before = runs(&app);

    // Right-click cancels.
    click(&mut app, Vec2::ZERO);
    button(&mut app, MouseButton::Right, true);
    app.update();
    button(&mut app, MouseButton::Right, false);
    app.update();
    assert!(app.world().resource::<ScriptTool>().points().is_empty());

    // Escape cancels too.
    click(&mut app, Vec2::ZERO);
    escape(&mut app, ButtonState::Pressed);
    app.update();
    escape(&mut app, ButtonState::Released);
    app.update();
    assert!(app.world().resource::<ScriptTool>().points().is_empty());

    // A fresh first point after a cancel starts a new gesture; it does not
    // complete the old one.
    click(&mut app, Vec2::new(2.0, 1.0));
    app.update();
    assert_eq!(runs(&app), before, "a cancelled gesture never runs the VM");
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn tool_reads_are_nan_outside_a_tool_run() {
    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit("(list (tool-count) (tool-x 0) (tool-y 0))");
    app.update();
    let entry = app.world().resource::<ScriptLog>().0.last().cloned();
    let entry = entry.expect("the read ran");
    assert!(entry.ok, "{}", entry.output);
    assert_eq!(entry.output.matches("nan").count(), 3, "{}", entry.output);
}

#[test]
fn a_bad_point_count_is_a_script_error() {
    let mut app = paused_app();
    for bad in ["0", "2.5", "1000"] {
        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit(format!("(register-tool \"t\" {bad} \"(undo)\")"));
        app.update();
        let entry = app.world().resource::<ScriptLog>().0.last().cloned();
        assert!(entry.is_some_and(|e| !e.ok), "{bad} points should fail");
    }
    assert!(app.world().resource::<ScriptTools>().0.is_empty());
}
