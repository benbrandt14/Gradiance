//! Adjust Last Operation (brief U-11): the last operation is recorded when
//! one call makes one undo step, and an adjustment replaces that step with
//! one made by the same operation with the edited params.

use crate::harness::{body_count, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::{CommandStack, HistoryInfo};
use gradiance::op::{InvokeOp, LastOp, Value, json};
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn invoke(app: &mut App, id: &'static str, params: Value) {
    app.world_mut().write_message(InvokeOp::with(id, params));
    step(app, 2);
}

fn script(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    step(app, 2);
}

fn adjust(app: &mut App, field: &str, value: Value) {
    invoke(
        app,
        "edit.adjust_last",
        json!({"field": field, "value": value}),
    );
}

fn last(app: &App) -> Option<(&'static str, Value)> {
    app.world()
        .resource::<LastOp>()
        .0
        .as_ref()
        .map(|r| (r.call.id, r.call.params.clone()))
}

fn depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

/// Every body's shape, sorted by x position.
fn shapes(app: &mut App) -> Vec<(f32, ShapeDef)> {
    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    let mut out: Vec<(f32, ShapeDef)> = q
        .iter(app.world())
        .map(|(s, t)| (t.translation.x, s.clone()))
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

#[test]
fn adjusting_a_spawn_replaces_its_step_and_undo_goes_back_before_it() {
    let mut app = paused_app();
    invoke(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 1, "w": 1, "h": 0.5}),
    );
    assert_eq!(body_count(&mut app), 1);
    let steps = depth(&app);
    assert_eq!(
        last(&app).map(|(id, _)| id),
        Some("body.spawn_box"),
        "the spawn is the last operation"
    );

    adjust(&mut app, "w", json!(2.5));
    assert_eq!(body_count(&mut app), 1, "the spawn was replaced, not added");
    assert_eq!(depth(&app), steps, "still one undo step");
    assert_eq!(
        shapes(&mut app)[0].1,
        ShapeDef::Box {
            width: 2.5,
            height: 0.5
        }
    );
    // The record follows the adjustment, so a second one composes.
    assert_eq!(last(&app).map(|(_, p)| p["w"].clone()), Some(json!(2.5)));
    adjust(&mut app, "h", json!(0.75));
    assert_eq!(
        shapes(&mut app)[0].1,
        ShapeDef::Box {
            width: 2.5,
            height: 0.75
        }
    );
    assert_eq!(depth(&app), steps);

    undo(&mut app);
    assert_eq!(
        body_count(&mut app),
        0,
        "undo restores the state before the spawn"
    );
    assert_eq!(last(&app), None, "an undo retires the last operation");
}

#[test]
fn adjust_after_an_undo_does_nothing() {
    let mut app = paused_app();
    invoke(
        &mut app,
        "body.spawn_circle",
        json!({"x": 0, "y": 1, "r": 0.5}),
    );
    invoke(
        &mut app,
        "body.spawn_circle",
        json!({"x": 3, "y": 1, "r": 0.5}),
    );
    undo(&mut app);
    let revision = app.world().resource::<CommandStack>().revision();
    adjust(&mut app, "r", json!(2.0));
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(
        shapes(&mut app)[0].1,
        ShapeDef::Circle { radius: 0.5 },
        "the remaining circle is untouched"
    );
    assert_eq!(
        app.world().resource::<CommandStack>().revision(),
        revision,
        "the history did not move"
    );
    assert_eq!(app.world().resource::<HistoryInfo>().redo_depth, 1);
}

#[test]
fn a_tool_gesture_that_is_no_operation_is_not_adjustable() {
    let mut app = paused_app();
    invoke(
        &mut app,
        "body.spawn_circle",
        json!({"x": 0, "y": 1, "r": 0.5}),
    );
    assert!(last(&app).is_some());
    // A drawn body commits its intent directly (the gesture path).
    app.world_mut().write_message(SpawnBodyIntent {
        record: crate::harness::box_record(Vec2::new(4.0, 1.0), 1.0, 1.0),
    });
    step(&mut app, 1);
    assert_eq!(last(&app), None, "a step no operation made retires it");
}

#[test]
fn a_script_line_and_a_fillet_merge_are_adjustable() {
    let mut app = paused_app();
    script(&mut app, "(spawn-circle 0 1 0.5)");
    assert_eq!(last(&app).map(|(id, _)| id), Some("body.spawn_circle"));
    adjust(&mut app, "x", json!(-0.3));
    script(&mut app, "(spawn-circle 0.4 1 0.5)");
    let steps = depth(&app);

    script(&mut app, "(fillet-merge 0 1 0.1)");
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(depth(&app), steps + 1);
    adjust(&mut app, "radius", json!(0.3));
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(depth(&app), steps + 1, "one step per adjustment");
    let shape = shapes(&mut app)[0].1.clone();
    let text = format!("{shape:?}");
    assert!(
        text.contains("SmoothUnion { radius: 0.3 }"),
        "the merge has the new radius: {text}"
    );
    undo(&mut app);
    assert_eq!(body_count(&mut app), 2, "undo restores both bodies");

    // A script run of two edits is a batch: nothing to adjust.
    script(
        &mut app,
        "(begin (spawn-circle 5 1 0.2) (spawn-circle 6 1 0.2))",
    );
    assert_eq!(last(&app), None);
}

#[test]
fn an_array_is_adjustable_by_count_and_step() {
    let mut app = paused_app();
    script(&mut app, "(spawn-box 0 1 1 1)");
    let steps = depth(&app);
    script(
        &mut app,
        "(array-copies (list 0) 2 (list 1.5 0) 0 (list 0 0))",
    );
    assert_eq!(body_count(&mut app), 3);
    adjust(&mut app, "count", json!(4));
    assert_eq!(body_count(&mut app), 5);
    adjust(&mut app, "step.0", json!(2.0));
    let xs: Vec<f32> = shapes(&mut app).iter().map(|(x, _)| *x).collect();
    assert!((xs[4] - 8.0).abs() < 1e-4, "four copies 2 m apart: {xs:?}");
    assert_eq!(depth(&app), steps + 1);
    undo(&mut app);
    assert_eq!(body_count(&mut app), 1);
}

#[test]
fn a_bad_adjustment_is_refused_and_the_step_stays() {
    let mut app = paused_app();
    invoke(
        &mut app,
        "body.spawn_circle",
        json!({"x": 0, "y": 1, "r": 0.5}),
    );
    let before = last(&app);
    // No such field: the operation refuses before anything moves.
    adjust(&mut app, "w", json!(1));
    // A radius the command refuses: undone, re-run, failed, redone.
    adjust(&mut app, "r", json!(-1));
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(shapes(&mut app)[0].1, ShapeDef::Circle { radius: 0.5 });
    assert_eq!(last(&app).map(|l| l.1), before.map(|l| l.1));
    assert_eq!(app.world().resource::<HistoryInfo>().redo_depth, 0);
}

#[test]
fn the_last_op_query_reports_the_call() {
    let mut app = paused_app();
    invoke(
        &mut app,
        "body.spawn_circle",
        json!({"x": 0, "y": 1, "r": 0.5}),
    );
    let ctx = gradiance::op::OpContext::build(app.world_mut());
    let entry = app
        .world()
        .resource::<gradiance::op::OpRegistry>()
        .get("edit.last_op")
        .cloned();
    let value = entry
        .and_then(|e| e.lower(json!({}), &ctx).ok())
        .and_then(|out| out.value().cloned());
    assert_eq!(
        value,
        Some(json!({"id": "body.spawn_circle", "params": {"x": 0, "y": 1, "r": 0.5}}))
    );
}
