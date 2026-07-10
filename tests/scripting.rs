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
use gradiance::script::bridge::ScriptInputs;
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
