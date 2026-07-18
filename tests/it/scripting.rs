//! Scripting as a scene-authoring surface, tested end-to-end.
//!
//! These drive the **full editor stack** (via the headless harness) with lisp
//! source: a script submits scene verbs, the exclusive `run_scripts` system
//! dispatches them through the intent bus, and the ordinary command path builds
//! the world — no special cases. This is the "scripts author tests" workflow:
//! a scene fixture is a few lines of lisp, and the assertions are on the real
//! authored world it produces.

use crate::harness::{body_count, paused_app, undo};
use bevy::prelude::*;
use gradiance::domain::settings::SimSettings;
use gradiance::script::bridge::{ScriptActions, ScriptInputs};

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
            (spawn-box 0 120 20 20)
            (spawn-ground 0 -100 0))",
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
