//! Scripting as a scene-authoring surface, tested end-to-end.
//!
//! These drive the **full editor stack** (via the headless harness) with lisp
//! source: a script submits scene verbs, the exclusive `run_scripts` system
//! dispatches them through the intent bus, and the ordinary command path builds
//! the world — no special cases. This is the "scripts author tests" workflow:
//! a scene fixture is a few lines of lisp, and the assertions are on the real
//! authored world it produces.

mod harness;

use bevy::prelude::*;
use gradiance::domain::settings::SimSettings;
use gradiance::script::bridge::{ScriptActions, ScriptInputs};
use harness::{body_count, paused_app, undo};

/// Submits `source` and advances one frame: `run_scripts` emits the intents,
/// `dispatch_intents` (later the same frame) turns them into commands.
fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

#[test]
fn a_script_spawns_a_body() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 40 20)");
    assert_eq!(body_count(&mut app), 1);
}

#[test]
fn a_script_authors_a_whole_scene() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin
            (spawn-box -60 0 40 20)
            (spawn-circle 60 0 15)
            (spawn-box 0 50 30 30))",
    );
    assert_eq!(body_count(&mut app), 3);
}

#[test]
fn scheme_control_flow_drives_authoring() {
    // A loop is a legitimate way to lay out a scene — five stacked boxes.
    let mut app = paused_app();
    run(
        &mut app,
        "(let loop ((i 0))
            (when (< i 5)
                (spawn-box 0 (* i 30) 20 20)
                (loop (+ i 1))))",
    );
    assert_eq!(body_count(&mut app), 5);
}

#[test]
fn a_scripted_cut_severs_a_body() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 40 20)");
    assert_eq!(body_count(&mut app), 1);
    // A stroke fully across the box at y = 0 severs it into two pieces.
    run(&mut app, "(cut -40 0 40 0 2)");
    assert_eq!(body_count(&mut app), 2);
}

#[test]
fn invalid_spawn_args_are_rejected_gracefully() {
    let mut app = paused_app();
    // Degenerate box (zero size): the spawn command validates the shape and
    // rejects it — no body, no panic.
    run(&mut app, "(spawn-box 0 0 0 0)");
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn a_syntax_error_leaves_the_world_untouched() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 40 20");
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn scripted_edits_are_undoable_commands() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 40 20)");
    assert_eq!(body_count(&mut app), 1);
    // A scripted spawn is one ordinary command — undo removes it.
    undo(&mut app);
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn a_script_reads_the_committed_scene() {
    // The read-total half of the governance model, end to end: a query builtin
    // reads the committed scene and drives a conditional edit. Reads observe
    // *last-committed* state (this frame's spawns are pending intents), so the
    // snapshot the loop sees is fixed at 2 throughout the run.
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box -50 0 20 20) (spawn-box 50 0 20 20))",
    );
    assert_eq!(body_count(&mut app), 2);
    run(
        &mut app,
        "(let loop ((i 0))
            (when (< i (body-count))
                (spawn-circle 0 (* i 40) 8)
                (loop (+ i 1))))",
    );
    // Two bodies were seen, so two circles were authored: 2 + 2 = 4.
    assert_eq!(body_count(&mut app), 4);
}

#[test]
fn count_at_reports_shape_containment() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 40 40)"); // covers [-20,20]^2
    // The origin is inside the box → the guard fires and a marker is authored.
    run(
        &mut app,
        "(when (> (count-at 0 0) 0) (spawn-circle 200 0 5))",
    );
    assert_eq!(body_count(&mut app), 2);
    // A far point is outside every shape → no edit.
    run(
        &mut app,
        "(when (> (count-at 500 500) 0) (spawn-circle 300 0 5))",
    );
    assert_eq!(body_count(&mut app), 2);
}

#[test]
fn body_accessors_read_positions() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 7 0 10 10)");
    // Only one body, so index 0 is it: author a circle at twice its x (14).
    run(&mut app, "(spawn-circle (* 2 (body-x 0)) 0 3)");
    assert_eq!(body_count(&mut app), 2);
    // Confirm the circle really landed at x = 14 (it, not the box, covers it).
    run(
        &mut app,
        "(when (> (count-at 14 0) 0) (spawn-box 0 100 5 5))",
    );
    assert_eq!(body_count(&mut app), 3);
}

#[test]
fn a_script_configures_the_simulation() {
    let mut app = paused_app();
    let gravity_y = |app: &mut App| app.world().resource::<SimSettings>().gravity.y;
    assert!((gravity_y(&mut app) - (-250.0)).abs() > 1.0); // default is -1000
    run(&mut app, "(sim-set \"gravity.y\" -250)");
    // Config is not authored state: the write lands on the settings resource
    // (the invariant-#4 seam), so it never touches the command stack.
    assert!((gravity_y(&mut app) - (-250.0)).abs() < 1e-3);
    // And an undo does not revert it (it was never a command).
    undo(&mut app);
    assert!((gravity_y(&mut app) - (-250.0)).abs() < 1e-3);
}

#[test]
fn a_script_reads_config_and_scene_together() {
    // sim-get and the scene queries compose: gravity points down, so drop a
    // marker per existing body. Demonstrates reads spanning both facades.
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box -30 0 10 10) (spawn-box 30 0 10 10))",
    );
    assert_eq!(body_count(&mut app), 2);
    run(
        &mut app,
        "(when (< (sim-get \"gravity.y\") 0)
            (let loop ((i 0))
                (when (< i (body-count))
                    (spawn-circle (* i 20) -80 4)
                    (loop (+ i 1)))))",
    );
    assert_eq!(body_count(&mut app), 4);
}

#[test]
fn a_registered_action_runs_when_invoked() {
    // The "add a menu action from a .scm file" loop end to end: a script
    // registers a named action, and invoking it — submitting its stored source,
    // exactly as the context menu does — authors the scene.
    let mut app = paused_app();
    run(
        &mut app,
        "(register-action \"Three boxes\"
            \"(begin (spawn-box -30 0 10 10) (spawn-box 0 0 10 10) (spawn-box 30 0 10 10))\")",
    );
    // Registering an action does not run it.
    assert_eq!(body_count(&mut app), 0);
    let source = app.world().resource::<ScriptActions>().0[0].source.clone();
    run(&mut app, &source);
    assert_eq!(body_count(&mut app), 3);
}

#[test]
fn the_op_catalog_is_introspectable_from_a_script() {
    // `(ops)` returns the registered op names and `(describe …)` their docs —
    // the homoiconic surface, observed by letting it drive an edit.
    let mut app = paused_app();
    run(
        &mut app,
        "(when (and (> (length (ops)) 0) (string? (describe \"cut\")))
            (spawn-box 0 0 10 10))",
    );
    assert_eq!(body_count(&mut app), 1);
}
