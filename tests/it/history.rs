//! Undo semantics that are not tied to a single command: auto-pause on
//! history navigation during a live run, and settings edits as undo steps.

#![allow(clippy::unwrap_used)]

use crate::harness::*;
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance_domain::settings::{LightingSettings, SimSettings};

/// Spawns a box and lets the stack settle, returning its id.
fn spawn_settled(app: &mut App) -> StableId {
    let record = box_record(Vec2::new(0.0, 500.0), 40.0, 40.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn state(app: &App) -> GameState {
    *app.world().resource::<State<GameState>>().get()
}

fn pos_of(app: &App, id: StableId) -> Vec2 {
    let entity = entity_of(app, id).unwrap();
    app.world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate()
}

// --- auto-pause on history navigation during a live run ------------------

#[test]
fn undo_during_a_live_run_pauses_and_reverts_the_run() {
    let mut app = paused_app();
    let id = spawn_settled(&mut app);
    let drawn = pos_of(&app, id);

    // Run: the body falls well away from where it was drawn.
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 120);
    let fallen = pos_of(&app, id);
    assert!(
        (fallen.y - drawn.y).abs() > 10.0,
        "sanity: the run should have moved the body ({drawn} -> {fallen})"
    );

    // One undo press, mid-run.
    undo(&mut app);

    assert_eq!(
        state(&app),
        GameState::Paused,
        "undo during a live run auto-pauses"
    );
    let after = pos_of(&app, id);
    assert!(
        (after - drawn).length() < 1.0,
        "the first press reverts the run, not an edit ({after} should be {drawn})"
    );
}

#[test]
fn auto_pause_leaves_no_drift_step_behind() {
    // Regression: the pause must land before physics steps again, or the
    // frames between undo and pause get recorded as a spurious "simulate"
    // step and a second undo reverts that instead of the edit.
    let mut app = paused_app();
    let first = spawn_settled(&mut app);
    let second = spawn_settled(&mut app);
    assert_eq!(body_count(&mut app), 2);

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 60);

    undo(&mut app); // reverts the run
    assert_eq!(
        body_count(&mut app),
        2,
        "the run is one step, not the spawn"
    );

    undo(&mut app); // must revert the second spawn, not a drift step
    assert_eq!(
        body_count(&mut app),
        1,
        "the next press reverts the edit; no drift step was recorded"
    );
    assert!(entity_of(&app, second).is_none());
    assert!(entity_of(&app, first).is_some());
}

#[test]
fn a_settled_run_that_moved_nothing_records_no_step() {
    // A static scene: running changes nothing, so undo should still reach
    // straight back to the edit.
    let mut app = paused_app();
    // Zero gravity first, and let that settle as its own step, so the only
    // thing under test is whether a motionless run adds one.
    app.world_mut().resource_mut::<SimSettings>().gravity = Vec2::ZERO;
    app.update();
    app.update();
    let id = spawn_settled(&mut app);

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 60);

    undo(&mut app);
    assert_eq!(
        body_count(&mut app),
        0,
        "a run that moved nothing adds no undo step"
    );
    assert!(entity_of(&app, id).is_none());
}

#[test]
fn redo_during_a_live_run_pauses_without_losing_the_branch() {
    let mut app = paused_app();
    let id = spawn_settled(&mut app);
    undo(&mut app);
    assert_eq!(body_count(&mut app), 0);

    // Start running with a redo branch pending, then redo mid-run.
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 30);
    redo(&mut app);

    assert_eq!(
        state(&app),
        GameState::Paused,
        "redo during a live run auto-pauses too"
    );
    assert_eq!(
        body_count(&mut app),
        1,
        "the redo branch survived the pause"
    );
    assert!(entity_of(&app, id).is_some());
}

// --- settings edits as undo steps ----------------------------------------

#[test]
fn a_settled_gravity_edit_is_one_undo_step() {
    let mut app = paused_app();
    spawn_settled(&mut app); // establish a baseline
    let original = app.world().resource::<SimSettings>().gravity;
    let edited = original + Vec2::new(0.0, 250.0);

    app.world_mut().resource_mut::<SimSettings>().gravity = edited;
    app.update(); // change seen: dirty
    app.update(); // settled: snapshot

    assert_eq!(app.world().resource::<SimSettings>().gravity, edited);
    undo(&mut app);
    assert_eq!(
        app.world().resource::<SimSettings>().gravity,
        original,
        "undo reverts a scene-content setting"
    );
    redo(&mut app);
    assert_eq!(app.world().resource::<SimSettings>().gravity, edited);
}

#[test]
fn a_drag_gesture_collapses_into_one_step() {
    let mut app = paused_app();
    spawn_settled(&mut app);
    let original = app.world().resource::<SimSettings>().gravity;

    // Ten frames of continuous change, as a slider drag produces.
    for i in 1..=10 {
        app.world_mut().resource_mut::<SimSettings>().gravity =
            original + Vec2::new(0.0, i as f32 * 10.0);
        app.update();
    }
    app.update(); // settles

    undo(&mut app);
    assert_eq!(
        app.world().resource::<SimSettings>().gravity,
        original,
        "the whole gesture is a single undo step"
    );
}

#[test]
fn workstation_config_stays_out_of_history() {
    let mut app = paused_app();
    spawn_settled(&mut app);

    let tweaked = {
        let mut grid = app.world_mut().resource_mut::<GridSettings>();
        grid.spacing += 25.0;
        grid.spacing
    };
    app.update();
    app.update();

    // Undo reverts the spawn — the grid edit never entered history at all.
    undo(&mut app);
    assert_eq!(body_count(&mut app), 0, "undo reached past the grid edit");
    assert!(
        (app.world().resource::<GridSettings>().spacing - tweaked).abs() < 1e-6,
        "undo never moves the grid out from under the user"
    );
}

#[test]
fn lighting_edits_are_undoable_too() {
    let mut app = paused_app();
    spawn_settled(&mut app);
    let original = app.world().resource::<LightingSettings>().clone();

    app.world_mut().resource_mut::<LightingSettings>().ambient += 0.25;
    app.update();
    app.update();

    undo(&mut app);
    assert_eq!(
        *app.world().resource::<LightingSettings>(),
        original,
        "lighting is scene content, so it is undoable"
    );
}
