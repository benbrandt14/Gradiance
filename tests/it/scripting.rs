//! Scripting as a scene-authoring surface, tested end-to-end.
//!
//! These drive the **full editor stack** (via the headless harness) with lisp
//! source: a script submits scene verbs, the exclusive `run_scripts` system
//! dispatches them through the intent bus, and the ordinary command path builds
//! the world — no special cases. This is the "scripts author tests" workflow:
//! a scene fixture is a few lines of lisp, and the assertions are on the real
//! authored world it produces.

use crate::harness::{body_count, joint_count, paused_app, undo};
use bevy::prelude::*;
use gradiance::domain::settings::SimSettings;
use gradiance::script::bridge::{
    PanelRequest, PanelRequests, PanelStates, ScriptActions, ScriptInputs,
};

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

/// Resizing: `(scale i fx fy)` works along the body's *own* axes about its own
/// centre, so "twice as wide" means what it says even for a rotated body and a
/// box stays a box.
#[test]
fn a_script_resizes_a_body_in_place() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 10 0 20 20)");
    run(&mut app, "(scale 0 2 0.5)");
    // The centre must not move — a pivot bug would show up as a shifted body.
    run(
        &mut app,
        "(begin
           (if (< (abs (- (body-x 0) 10)) 0.001) (spawn-box 0 -100 4 4) 0))",
    );
    assert_eq!(body_count(&mut app), 2, "scaled in place, centre unmoved");
    undo(&mut app);
    undo(&mut app);
    assert_eq!(
        body_count(&mut app),
        1,
        "the scale was one undoable command"
    );
}

/// A zero or negative factor is not a resize: zero is unrecoverable and a
/// negative mirrors. Both are rejected rather than committed.
#[test]
fn a_degenerate_scale_factor_is_rejected() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(
        &mut app,
        "(begin (scale 0 0 1) (scale 0 -2 1) (scale 0 1 0))",
    );
    // Nothing was emitted, so undo reaches the spawn.
    undo(&mut app);
    assert_eq!(body_count(&mut app), 0, "no degenerate scale was committed");
}

/// `(merge a b)` is what "weld" means here — one CSG union, `a` surviving with
/// both shapes. It is the relationship that *removes* a body.
#[test]
fn a_script_merges_two_bodies_into_one() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 20 20) (spawn-box 15 0 20 20))",
    );
    assert_eq!(body_count(&mut app), 2);
    run(&mut app, "(merge 0 1)");
    assert_eq!(body_count(&mut app), 1, "two bodies became one");
    undo(&mut app);
    assert_eq!(body_count(&mut app), 2, "and the merge was undoable");
}

/// Merging a body with itself would ask the command to fuse one body into one
/// body — a no-op that would still cost an undo step.
#[test]
fn merging_a_body_with_itself_does_nothing() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(&mut app, "(merge 0 0)");
    undo(&mut app);
    assert_eq!(
        body_count(&mut app),
        0,
        "no step, so undo reached the spawn"
    );
}

/// A script could *make* relationships but not unmake one individually — only
/// undo them wholesale. `(delete-joint i)` closes that, indexed like the bodies.
#[test]
fn a_script_removes_one_joint_of_several() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 20 6) (spawn-box 30 0 20 6) (spawn-box 60 0 20 6))",
    );
    run(&mut app, "(begin (hinge 0 1 15 0) (hinge 1 2 45 0))");
    assert_eq!(joint_count(&mut app), 2);

    run(&mut app, "(delete-joint 0)");
    assert_eq!(joint_count(&mut app), 1, "one joint removed, one left");
    undo(&mut app);
    assert_eq!(joint_count(&mut app), 2, "and it came back");

    // An index past the end removes nothing.
    run(&mut app, "(delete-joint 99)");
    assert_eq!(joint_count(&mut app), 2);
}

/// Body properties round-trip: a `set-*` verb writes through the same
/// `PropertyEditIntent` the inspector's fields commit, and the matching
/// `body-*` read gets it back. Reads are total; writes are seam-mediated — this
/// asserts both halves name the same quantity.
#[test]
fn body_properties_round_trip_through_the_property_seam() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(
        &mut app,
        "(begin (set-friction 0 0.8) (set-restitution 0 0.25) (set-density 0 3))",
    );
    // Read each back and spawn a marker per value that took.
    run(
        &mut app,
        "(begin
           (if (> (body-friction 0) 0.79) (spawn-box 0 -100 4 4) 0)
           (if (< (body-restitution 0) 0.26) (spawn-box 10 -100 4 4) 0)
           (if (> (body-density 0) 2.9) (spawn-box 20 -100 4 4) 0))",
    );
    assert_eq!(body_count(&mut app), 4, "all three values read back");
}

/// `(set-static i on)` is one verb with an argument rather than two verbs that
/// could disagree, and it is undoable like any property edit.
#[test]
fn making_a_body_static_is_reversible() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(
        &mut app,
        "(if (> (body-static? 0) 0) (spawn-box 0 -100 4 4) 0)",
    );
    assert_eq!(body_count(&mut app), 1, "a fresh box is dynamic");

    run(&mut app, "(set-static 0 1)");
    run(
        &mut app,
        "(if (> (body-static? 0) 0) (spawn-box 0 -100 4 4) 0)",
    );
    assert_eq!(body_count(&mut app), 2, "now static");

    // Undo the marker spawn, then the static edit.
    undo(&mut app);
    undo(&mut app);
    run(
        &mut app,
        "(if (> (body-static? 0) 0) (spawn-box 0 -100 4 4) 0)",
    );
    assert_eq!(body_count(&mut app), 1, "dynamic again, no marker");

    // And zero puts it back explicitly — in a *separate* run, see below.
    run(&mut app, "(set-static 0 1)");
    run(&mut app, "(set-static 0 0)");
    run(
        &mut app,
        "(if (> (body-static? 0) 0) (spawn-box 0 -100 4 4) 0)",
    );
    assert_eq!(body_count(&mut app), 1, "0 means dynamic");
}

/// The one-snapshot-per-run rule reaches property writes too, and here it is
/// genuinely surprising: `(begin (set-static 0 1) (set-static 0 0))` leaves the
/// body **static**, because both calls read the same pre-run value — so the
/// second sees `old == new` and is correctly suppressed as a no-op.
///
/// This is the documented semantics, not a bug, and it is pinned because the
/// naive reading (that the calls compose left to right) is the one a script
/// author will reach for first.
#[test]
fn property_writes_in_one_run_all_see_the_pre_run_value() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(&mut app, "(begin (set-static 0 1) (set-static 0 0))");
    run(
        &mut app,
        "(if (> (body-static? 0) 0) (spawn-box 0 -100 4 4) 0)",
    );
    assert_eq!(
        body_count(&mut app),
        2,
        "the second set saw the pre-run value and was a no-op, so the body is static"
    );

    // The same shape with two *different* properties composes fine, because
    // each reads a field the other does not touch.
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(&mut app, "(begin (set-friction 0 0.9) (set-density 0 5))");
    run(
        &mut app,
        "(begin
           (if (> (body-friction 0) 0.89) (spawn-box 0 -100 4 4) 0)
           (if (> (body-density 0) 4.9) (spawn-box 10 -100 4 4) 0))",
    );
    assert_eq!(body_count(&mut app), 3, "independent fields both took");
}

/// Setting a property to the value it already holds must not push an empty undo
/// step — the same guard `place` has, and easy to get wrong because the write
/// still "succeeds".
#[test]
fn setting_a_property_to_its_current_value_is_not_a_step() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(&mut app, "(set-friction 0 0.5)");
    // 0.5 is the default, so that was a no-op; undo should reach the spawn.
    undo(&mut app);
    assert_eq!(
        body_count(&mut app),
        0,
        "the redundant set left no step, so undo reached the spawn"
    );
}

/// Reads of a body that does not exist are NaN rather than a panic or a zero
/// that would read as a real measurement.
#[test]
fn property_reads_of_a_missing_body_are_not_silently_zero() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    // NaN compares false against everything, so no marker spawns either way.
    run(
        &mut app,
        "(begin
           (if (> (body-friction 99) -1000) (spawn-box 0 -100 4 4) 0)
           (if (< (body-density 99) 1000) (spawn-box 10 -100 4 4) 0))",
    );
    assert_eq!(body_count(&mut app), 1, "NaN failed both comparisons");
    // And a write to a missing body changes nothing.
    run(&mut app, "(set-friction 99 0.9)");
    run(
        &mut app,
        "(if (> (body-friction 0) 0.89) (spawn-box 0 -100 4 4) 0)",
    );
    assert_eq!(body_count(&mut app), 1, "body 0 was not touched");
}

/// A script could spawn and delete but never *move* anything after the fact.
/// `(place …)` closes that, and it is one undo step because the old pose comes
/// from the run's snapshot — a command that only knew the new pose could not
/// reverse itself.
#[test]
fn a_script_moves_and_rotates_a_body() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(&mut app, "(place 0 50 25 1.5)");
    // Read the pose back through the query verbs — the same index vocabulary.
    run(
        &mut app,
        "(begin
             (if (> (body-x 0) 49) (spawn-box 0 -100 4 4) 0)
             (if (> (body-y 0) 24) (spawn-box 10 -100 4 4) 0)
             (if (> (body-rot 0) 1.4) (spawn-box 20 -100 4 4) 0))",
    );
    assert_eq!(
        body_count(&mut app),
        4,
        "x, y and rotation all took — three markers spawned"
    );
}

#[test]
fn a_scripted_move_is_undoable_and_a_no_op_move_is_not_a_step() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 20)");
    run(&mut app, "(place 0 50 0 0)");
    undo(&mut app);
    // Back at the origin: a marker spawns only if x came back below 1.
    run(&mut app, "(if (< (body-x 0) 1) (spawn-box 0 -100 4 4) 0)");
    assert_eq!(body_count(&mut app), 2, "the move was undoable");

    // Placing a body where it already is must not push an empty undo step:
    // undoing after it should reverse the *spawn*, not nothing.
    let mut app = paused_app();
    run(&mut app, "(spawn-box 7 0 20 20)");
    run(&mut app, "(place 0 7 0 0)");
    undo(&mut app);
    assert_eq!(
        body_count(&mut app),
        0,
        "the no-op place left no step, so undo reached the spawn"
    );
}

/// A **chain** — the thing a multibody DSL exists for, and the thing a script
/// could not express at all before: three bodies, two hinges, authored in one
/// run. Joints are the *relationships* half of the scene model.
#[test]
fn a_script_builds_a_hinged_chain() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 20 6) (spawn-box 30 0 20 6) (spawn-box 60 0 20 6))",
    );
    assert_eq!(body_count(&mut app), 3);
    // Hinge 0-1 at their shared edge, then 1-2 at theirs.
    run(&mut app, "(begin (hinge 0 1 15 0) (hinge 1 2 45 0))");
    assert_eq!(joint_count(&mut app), 2, "two hinges in the scene");
}

/// A joint verb returns its handle, and `b < 0` is the world pin — the
/// one-body case the tools produce by clicking where only one body sits.
#[test]
fn a_negative_second_body_pins_to_the_world() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 6)");
    run(&mut app, "(hinge 0 -1 0 0)");
    assert_eq!(joint_count(&mut app), 1);
    // The handle is a real value, so `(define j (hinge …))` names the joint —
    // asserted by binding it and using it, since a verb that returned an empty
    // string would still have spawned the joint above.
    run(
        &mut app,
        r#"(begin (define j (hinge 0 -1 10 0)) (label j "pivot"))"#,
    );
    assert_eq!(joint_count(&mut app), 2);
    let labels = app
        .world()
        .resource::<gradiance::script::bridge::WorkspaceLabels>();
    assert_eq!(
        labels.0.first().map(|(n, _)| n.as_str()),
        Some("pivot"),
        "the joint handle parsed as an id and bound a name"
    );
}

/// All three kinds reach the scene, and each is one undoable command. A joint
/// that skipped the command seam would look identical until someone pressed
/// undo — the same trap the delete test guards.
#[test]
fn every_joint_kind_lands_and_is_undoable() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 20 6) (spawn-box 40 0 20 6))",
    );
    run(&mut app, "(hinge 0 1 20 0)");
    run(&mut app, "(slider 0 1 20 0 1 0)");
    run(&mut app, "(spring 0 1 100 0.5)");
    assert_eq!(joint_count(&mut app), 3, "hinge, slider, spring");

    undo(&mut app);
    assert_eq!(joint_count(&mut app), 2, "the spring was one command");
    undo(&mut app);
    undo(&mut app);
    assert_eq!(joint_count(&mut app), 0);
}

/// Bad body indices must not produce a half-built joint. A first index that
/// does not resolve emits nothing at all; a bad *second* index degrades to a
/// world pin, so a typo shows up in the scene rather than silently vanishing.
#[test]
fn joint_verbs_reject_unresolvable_bodies() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 6)");

    run(&mut app, "(hinge 99 0 0 0)");
    assert_eq!(joint_count(&mut app), 0, "no body 99 — nothing emitted");
    run(&mut app, "(hinge -1 0 0 0)");
    assert_eq!(
        joint_count(&mut app),
        0,
        "a negative *first* body is not a pin"
    );

    run(&mut app, "(hinge 0 99 0 0)");
    assert_eq!(
        joint_count(&mut app),
        1,
        "a bad second body pins to the world"
    );
}

/// A hinge between a body and *itself* is meaningless, and avian would be
/// constraining one body to its own anchor. It degrades to a world pin.
#[test]
fn a_joint_from_a_body_to_itself_becomes_a_world_pin() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 20 6)");
    run(&mut app, "(hinge 0 0 0 0)");
    assert_eq!(joint_count(&mut app), 1, "one joint, not a self-constraint");
}

/// The history verbs are the Edit menu's undo/redo as ops: no arguments, and
/// they route through the *same* `UndoIntent`/`RedoIntent` the menu emits, so a
/// script can walk the stack it just wrote to.
#[test]
fn a_script_can_undo_and_redo_its_own_edits() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 10 10)");
    run(&mut app, "(spawn-box 30 0 10 10)");
    assert_eq!(body_count(&mut app), 2);

    run(&mut app, "(undo)");
    assert_eq!(body_count(&mut app), 1, "one step back");
    run(&mut app, "(redo)");
    assert_eq!(body_count(&mut app), 2, "and forward again");
}

/// `(delete i)` is the destructive counterpart to `spawn-*`, indexed the same
/// way `body-x` is. It goes through `DeleteIntent`, so it is one undoable
/// command — asserted here, because a delete that skipped the command seam
/// would look identical until someone pressed undo.
#[test]
fn a_scripted_delete_is_one_undoable_command() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 10 10) (spawn-box 30 0 10 10))",
    );
    assert_eq!(body_count(&mut app), 2);

    run(&mut app, "(delete 0)");
    assert_eq!(body_count(&mut app), 1);
    undo(&mut app);
    assert_eq!(body_count(&mut app), 2, "the delete was undoable");
}

/// An out-of-range or negative index must be a no-op, not a panic and not a
/// delete of something else. The index is resolved against the run's snapshot,
/// so this is the boundary every scripted edit shares.
#[test]
fn a_delete_with_a_bad_index_does_nothing() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 10 10)");
    run(&mut app, "(begin (delete 99) (delete -1))");
    assert_eq!(body_count(&mut app), 1);
}

/// Within **one** run every index resolves against the same snapshot, taken
/// before the script started. Two different indices therefore delete two
/// bodies (what you would expect), and the same index twice emits two intents
/// for one id — the second must be a harmless no-op at dispatch, not a panic.
/// This is the seam's sharpest edge, so it is pinned rather than assumed.
#[test]
fn indices_in_one_run_resolve_against_one_snapshot() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 10 10) (spawn-box 30 0 10 10) (spawn-box 60 0 10 10))",
    );
    // Two distinct indices in one run: both land.
    run(&mut app, "(begin (delete 0) (delete 1))");
    assert_eq!(body_count(&mut app), 1);

    // The same index twice in one run: one body goes, the repeat is inert.
    run(
        &mut app,
        "(begin (spawn-box 90 0 10 10) (spawn-box 120 0 10 10))",
    );
    assert_eq!(body_count(&mut app), 3);
    run(&mut app, "(begin (delete 0) (delete 0))");
    assert_eq!(
        body_count(&mut app),
        2,
        "a repeated index deletes once and does not panic"
    );
}

/// Reads compose with the new edit: clear the scene by deleting index 0 as many
/// times as there are bodies. Each `(delete 0)` resolves against a fresh
/// snapshot, which is what makes the loop terminate at the right count.
#[test]
fn delete_composes_with_the_query_verbs() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 10 10) (spawn-box 30 0 10 10) (spawn-box 60 0 10 10))",
    );
    assert_eq!(body_count(&mut app), 3);
    for _ in 0..3 {
        run(&mut app, "(when (> (body-count) 0) (delete 0))");
    }
    assert_eq!(body_count(&mut app), 0);
}

/// The panel verbs queue a request rather than writing panel state — the UI
/// layer sits above the script layer, so it could not write it even if the
/// seam allowed. This asserts the script half: three verbs, three queued
/// requests, in order, with `None` meaning "flip".
#[test]
fn panel_verbs_queue_requests_for_the_ui_to_apply() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (panel-show \"properties\") (panel-hide \"console\") (panel-toggle \"plot\"))",
    );
    let queued = &app.world().resource::<PanelRequests>().0;
    assert_eq!(
        queued,
        &[
            PanelRequest {
                name: "properties".to_owned(),
                shown: Some(true),
            },
            PanelRequest {
                name: "console".to_owned(),
                shown: Some(false),
            },
            PanelRequest {
                name: "plot".to_owned(),
                shown: None,
            },
        ]
    );
}

/// A name is normalised (trimmed, lower-cased) on the way in, so
/// `(panel-show "  Properties ")` is not a silent no-op.
#[test]
fn panel_names_are_normalised_not_taken_literally() {
    let mut app = paused_app();
    run(&mut app, "(panel-show \"  Properties \")");
    assert_eq!(
        app.world().resource::<PanelRequests>().0[0].name,
        "properties"
    );
}

/// `panel-open?` reads the mirror the UI publishes. Headless there is no UI, so
/// this seeds the mirror directly — which is exactly the contract: the verb
/// reads `PanelStates` and nothing else, and an unknown panel is `#f` rather
/// than an error that would abort the run.
#[test]
fn panel_open_reads_the_published_mirror() {
    let mut app = paused_app();
    app.world_mut().resource_mut::<PanelStates>().0 = vec![
        ("properties".to_owned(), true),
        ("console".to_owned(), false),
    ];
    // Spawn one box per open panel the script finds: an observable count.
    run(
        &mut app,
        "(begin
           (if (panel-open? \"properties\") (spawn-box 0 0 10 10) 0)
           (if (panel-open? \"console\") (spawn-box 20 0 10 10) 0)
           (if (panel-open? \"nope\") (spawn-box 40 0 10 10) 0))",
    );
    assert_eq!(
        body_count(&mut app),
        1,
        "only `properties` was open; a closed and an unknown panel are both #f"
    );
}

/// The UI half: `apply_panel_requests` resolves a name against the registry,
/// applies it through `PanelToggle`, and clears the queue.
///
/// This runs the system on a bare `App` with just the panel resources, because
/// the real UI plugin no-ops without a renderer. It is the half the scripting
/// test above cannot reach, and it covers the failure mode that matters: an
/// unknown name must not touch anything, and must not stop the rest of the
/// batch.
#[test]
fn the_ui_applies_queued_panel_requests_and_ignores_unknown_names() {
    use gradiance::ui::panels::{PanelToggle, apply_panel_requests};

    let mut app = App::new();
    app.init_resource::<PanelRequests>();
    app.init_resource::<gradiance::ui::settings::SettingsWindow>();
    app.init_resource::<gradiance::ui::inspector::InspectorPanel>();
    app.init_resource::<gradiance::ui::depth_panel::DepthPanel>();
    app.init_resource::<gradiance::ui::plot::PlotPanel>();
    app.init_resource::<gradiance::ui::probe::ProbePanel>();
    app.init_resource::<gradiance::ui::node_graph::NodeGraph>();
    app.init_resource::<gradiance::ui::outliner::ObjectTreePanel>();
    app.init_resource::<gradiance::ui::console::ScriptConsole>();
    app.init_resource::<gradiance::ui::array_panel::ArrayWindow>();
    app.init_resource::<gradiance::ui::optimizer::OptimizerExpanded>();
    app.init_resource::<gradiance::domain::settings::DebugSettings>();
    app.add_systems(Update, apply_panel_requests);

    app.world_mut().resource_mut::<PanelRequests>().0 = vec![
        PanelRequest {
            name: "properties".to_owned(),
            shown: Some(true),
        },
        PanelRequest {
            name: "not-a-panel".to_owned(),
            shown: Some(true),
        },
        // Queued after the bad name: it must still be applied.
        PanelRequest {
            name: "depth".to_owned(),
            shown: Some(true),
        },
    ];
    app.update();

    assert!(
        app.world()
            .resource::<gradiance::ui::inspector::InspectorPanel>()
            .is_open()
    );
    assert!(
        app.world()
            .resource::<gradiance::ui::depth_panel::DepthPanel>()
            .is_open(),
        "an unknown name must not abort the batch"
    );
    assert!(
        app.world().resource::<PanelRequests>().0.is_empty(),
        "the queue is drained, so a request applies once and not every frame"
    );

    // And `None` flips rather than setting.
    app.world_mut().resource_mut::<PanelRequests>().0 = vec![PanelRequest {
        name: "depth".to_owned(),
        shown: None,
    }];
    app.update();
    assert!(
        !app.world()
            .resource::<gradiance::ui::depth_panel::DepthPanel>()
            .is_open()
    );
}

#[test]
fn spawn_ground_authors_a_body() {
    let mut app = paused_app();
    run(&mut app, "(spawn-ground 0 -200 0)");
    assert_eq!(body_count(&mut app), 1);
    // A floor plus a box on top: the two-line scene-setup a fixture wants.
    run(&mut app, "(spawn-box 0 0 20 20)");
    assert_eq!(body_count(&mut app), 2);
}

#[test]
fn nearest_dist_and_index_at_compose_with_edits() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box -50 0 20 20) (spawn-box 50 0 20 20))",
    );
    assert_eq!(body_count(&mut app), 2);
    // Nearest centre to (-40, 0) is the left box at distance 10 (< 60) → marker.
    run(
        &mut app,
        "(when (< (nearest-dist -40 0) 60) (spawn-circle 0 100 5))",
    );
    assert_eq!(body_count(&mut app), 3);
    // (-50, 0) is inside the left box → a real index (>= 0) → marker.
    run(
        &mut app,
        "(when (>= (body-index-at -50 0) 0) (spawn-circle 0 200 5))",
    );
    assert_eq!(body_count(&mut app), 4);
    // Empty space → -1 → no marker.
    run(
        &mut app,
        "(when (>= (body-index-at 999 999) 0) (spawn-circle 0 300 5))",
    );
    assert_eq!(body_count(&mut app), 4);
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

#[test]
fn signal_verbs_publish_and_read_the_bus() {
    use gradiance::signal::SignalBus;
    let mut app = paused_app();
    run(&mut app, "(signal-set \"excitement\" 7)");
    assert_eq!(
        app.world().resource::<SignalBus>().get("excitement"),
        Some(7.0),
        "signal-set lands on the bus"
    );
    // signal-get reads the per-run mirror: derive a second signal from it.
    run(
        &mut app,
        "(signal-set \"doubled\" (* 2 (signal-get \"excitement\")))",
    );
    assert_eq!(
        app.world().resource::<SignalBus>().get("doubled"),
        Some(14.0)
    );
}

#[test]
fn a_script_drives_color_from_touch_count() {
    // The user story: a script computes how many bodies a body touches and
    // publishes it; a Named binding turns that into the body's color.
    use gradiance::signal::{
        SignalBinding, SignalBindings, SignalBus, SignalColorOverride, SignalMap, SignalSink,
        SignalSource,
    };
    let mut app = crate::harness::headless_app();
    run(
        &mut app,
        "(begin
            (spawn-box 0 1.2 0.2 0.2)
            (spawn-ground 0 -1 0))",
    );
    let boxes: Vec<gradiance::core::ids::StableId> = app
        .world_mut()
        .query_filtered::<(
            &gradiance::core::ids::StableId,
            &gradiance::domain::shape::ShapeDef,
        ), bevy::prelude::With<gradiance::domain::Body>>()
        .iter(app.world())
        .filter(|(_, shape)| !shape.contains_half_plane())
        .map(|(id, _)| *id)
        .collect();
    let box_id = boxes[0];
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "touch-fill".into(),
            source: SignalSource::Named("touches".into()),
            map: SignalMap {
                in_min: 0.0,
                in_max: 4.0,
            },
            curve: None,
            gradient: gradiance::signal::GradientSpec::default(),
            sink: SignalSink::Fill(box_id),
        });

    crate::harness::step(&mut app, 180); // fall and rest on the ground
    // The box is the body with the smaller id-ordered index of the two; find
    // its index via touch-count over both and take the max.
    run(
        &mut app,
        "(signal-set \"touches\" (max (touch-count 0) (touch-count 1)))",
    );
    crate::harness::step(&mut app, 2);

    assert!(
        app.world().resource::<SignalBus>().get("touches").unwrap() >= 1.0,
        "the script read a real touch count"
    );
    let entity = crate::harness::entity_of(&app, box_id).unwrap();
    assert!(
        app.world()
            .get::<SignalColorOverride>(entity)
            .is_some_and(|o| o.fill.is_some()),
        "the script-published count drives the body's color"
    );
}

#[test]
fn spawn_verbs_return_workspace_handles() {
    // `(define b (spawn-box …))` binds the body's handle; `(label b name)`
    // makes it a workspace name resolving to the real StableId.
    let mut app = paused_app();
    run(
        &mut app,
        r#"(begin
            (define b (spawn-box 0 0 40 20))
            (label b "crate"))"#,
    );
    assert_eq!(body_count(&mut app), 1);
    let labels = app
        .world()
        .resource::<gradiance::script::bridge::WorkspaceLabels>();
    let (name, id) = labels.0.first().expect("one workspace label");
    assert_eq!(name, "crate");
    let entity = app
        .world()
        .resource::<gradiance::core::ids::IdIndex>()
        .entity(*id);
    assert!(entity.is_some(), "the label resolves to the spawned body");
    assert_eq!(labels.name_of(*id), Some("crate"));
}

#[test]
fn relabelling_a_name_rebinds_it() {
    let mut app = paused_app();
    run(&mut app, r#"(label (spawn-circle 0 0 10) "ball")"#);
    run(&mut app, r#"(label (spawn-circle 50 0 10) "ball")"#);
    let labels = app
        .world()
        .resource::<gradiance::script::bridge::WorkspaceLabels>();
    assert_eq!(labels.0.len(), 1, "labels are unique by name");
    assert_eq!(body_count(&mut app), 2, "both bodies still exist");
}

#[test]
fn ans_carries_the_last_value_between_runs() {
    // MATLAB cue: each run's last value binds to `ans`, and the log echoes
    // it instead of a bare "ok".
    let mut app = paused_app();
    run(&mut app, "(+ 1 2)");
    {
        let log = app
            .world()
            .resource::<gradiance::script::bridge::ScriptLog>();
        let entry = log.0.last().expect("logged");
        assert!(entry.ok);
        assert_eq!(entry.output, "3", "the value is echoed");
    }
    run(&mut app, "(* ans 14)");
    let log = app
        .world()
        .resource::<gradiance::script::bridge::ScriptLog>();
    let entry = log.0.last().expect("logged");
    assert!(entry.ok, "ans resolves in the next run: {}", entry.output);
    assert_eq!(entry.output, "42");
}

#[test]
fn defparam_and_defsignal_build_the_dataflow() {
    use gradiance::signal::{ComputedSignals, SignalBus, SignalParams};
    let mut app = paused_app();
    run(&mut app, "(defparam \"amp\" 2 0 10)");
    run(&mut app, "(defsignal \"osc\" \"amp 3 *\")");

    // The declarations landed on the config resources.
    let params = app.world().resource::<SignalParams>();
    assert_eq!(params.0.len(), 1);
    assert_eq!(params.0[0].name, "amp");
    assert!((params.0[0].value - 2.0).abs() < 1e-6);
    assert_eq!(app.world().resource::<ComputedSignals>().0.len(), 1);

    // And the evaluator computes them: amp=2 → osc = 2*3 = 6.
    crate::harness::step(&mut app, 2);
    let bus = app.world().resource::<SignalBus>();
    assert_eq!(bus.get("amp"), Some(2.0));
    assert_eq!(bus.get("osc"), Some(6.0));

    // Re-running defparam upserts (updates in place, no duplicate).
    run(&mut app, "(defparam \"amp\" 5 0 10)");
    assert_eq!(app.world().resource::<SignalParams>().0.len(), 1);
    crate::harness::step(&mut app, 2);
    assert_eq!(app.world().resource::<SignalBus>().get("osc"), Some(15.0));
}
