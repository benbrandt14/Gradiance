//! Undo semantics that are not tied to a single command: auto-pause on
//! history navigation during a live run, and settings edits as undo steps.

#![allow(clippy::unwrap_used)]

use crate::harness::*;
use avian2d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::prelude::*;
use gradiance::signal::{
    ComputedSignal, ComputedSignals, GradientSpec, SignalBinding, SignalBindings, SignalExpr,
    SignalMap, SignalParam, SignalParams, SignalSink, SignalSource,
};
use gradiance_domain::settings::{CameraSettings, LightingSettings, SimSettings};

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

    // One undo press, mid-run. The pause is a state transition, so it lands
    // at the start of the next frame (before physics, which is why nothing
    // drifts in between).
    undo(&mut app);
    app.update();

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

/// Regression (brief Q-1, found on screen): undoing a run put each body
/// back at its pre-run pose but left the run's velocity on it, so the next
/// play launched it (a thruster box read 62 m/s while paused). A restored
/// pose is a pose at rest.
#[test]
fn undoing_a_run_leaves_the_bodies_at_rest() {
    let mut app = paused_app();
    let id = spawn_settled(&mut app);
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 30);
    let entity = entity_of(&app, id).unwrap();
    let falling = app.world().get::<LinearVelocity>(entity).unwrap().0;
    assert!(
        falling.length() > 1.0,
        "sanity: the body is falling ({falling})"
    );

    undo(&mut app);
    app.update();

    let entity = entity_of(&app, id).unwrap();
    let v = app.world().get::<LinearVelocity>(entity).unwrap().0;
    let w = app.world().get::<AngularVelocity>(entity).unwrap().0;
    assert!(
        v.length() < 1e-6 && w.abs() < 1e-6,
        "an undone run leaves no motion behind (v {v}, w {w})"
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

/// Regression (brief 6-1, found by the loader mutation test): pausing
/// took one more physics step on the pausing frame, after the run had been
/// recorded, so the "simulate" step held a pose one step behind where the
/// bodies stopped, and undoing a later edit moved them back by that step.
#[test]
fn a_paused_run_records_the_pose_the_bodies_stop_at() {
    let mut app = paused_app();
    let id = spawn_settled(&mut app);
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 10);
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.update();
    step(&mut app, 3);
    let stopped = pos_of(&app, id);
    // Back to the drawn pose and forward again: redo restores the recorded
    // run, which must be where the body stopped.
    undo(&mut app);
    assert_ne!(pos_of(&app, id), stopped, "sanity: the run moved the body");
    redo(&mut app);
    assert_eq!(
        pos_of(&app, id),
        stopped,
        "the recorded run is the pose the body stopped at"
    );
}

#[test]
fn a_settled_run_that_moved_nothing_records_no_step() {
    // A static scene: running changes nothing, so undo should still reach
    // straight back to the edit.
    let mut app = paused_app();
    // Zero gravity first, and let that settle as its own step, so the only
    // thing under test is whether a motionless run adds one.
    app.world_mut().resource_mut::<SimSettings>().gravity =
        gradiance::units::Acceleration2(Vec2::ZERO);
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
    app.update(); // let the pause transition land

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
    let edited = original + gradiance::units::Acceleration2(Vec2::new(0.0, 250.0));

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
            original + gradiance::units::Acceleration2(Vec2::new(0.0, i as f32 * 10.0));
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

/// The gravity-direction widget edits `(angle, magnitude)` and writes the
/// vector back through `SimSettings::gravity_from_polar`; a whole dial drag
/// (many frames of polar edits) settles into one undo step. The egui dial
/// itself is not driven headless — this pins the seam it writes through.
#[test]
fn a_gravity_direction_drag_is_one_undo_step() {
    let mut app = paused_app();
    spawn_settled(&mut app);
    let original = app.world().resource::<SimSettings>().gravity;
    let (angle, magnitude) = app.world().resource::<SimSettings>().gravity_polar();

    // Sweep the dial a quarter turn, then raise the strength.
    for i in 1..=10 {
        let a = angle + i as f32 * 9.0;
        app.world_mut().resource_mut::<SimSettings>().gravity =
            SimSettings::gravity_from_polar(a, magnitude);
        app.update();
    }
    app.world_mut().resource_mut::<SimSettings>().gravity =
        SimSettings::gravity_from_polar(angle + 90.0, magnitude * 2.0);
    app.update();
    app.update(); // settles
    let aimed = app.world().resource::<SimSettings>().gravity;
    let aimed = aimed.value();
    assert!((aimed - Vec2::new(20.0, 0.0)).length() < 1e-3, "{aimed}");

    undo(&mut app);
    let back = app.world().resource::<SimSettings>().gravity;
    assert!(
        (back - original).value().length() < 1e-5,
        "the whole dial gesture is one undo step ({back:?} vs {original:?})"
    );
    assert_eq!(
        body_count(&mut app),
        1,
        "the undo stopped at the gravity edit"
    );
}

#[test]
fn workstation_config_stays_out_of_history() {
    let mut app = paused_app();
    spawn_settled(&mut app);

    let tweaked = {
        let mut grid = app.world_mut().resource_mut::<GridSettings>();
        grid.spacing = grid.spacing + gradiance::units::Length(25.0);
        grid.spacing
    };
    app.update();
    app.update();

    // Undo reverts the spawn — the grid edit never entered history at all.
    undo(&mut app);
    assert_eq!(body_count(&mut app), 0, "undo reached past the grid edit");
    assert!(
        (app.world().resource::<GridSettings>().spacing - tweaked)
            .value()
            .abs()
            < 1e-6,
        "undo never moves the grid out from under the user"
    );
}

#[test]
fn camera_settings_are_workstation_config() {
    let mut app = paused_app();
    spawn_settled(&mut app);

    let tweaked = CameraSettings {
        zoom_step: 0.7,
        pan_speed: 123.0,
        orbit_speed_deg: 1.0,
        arrow_scale: 0.5,
    };
    *app.world_mut().resource_mut::<CameraSettings>() = tweaked.clone();
    app.update();
    app.update();

    // Undo reverts the spawn — the camera edit never entered history.
    undo(&mut app);
    assert_eq!(body_count(&mut app), 0, "undo reached past the camera edit");
    assert_eq!(
        *app.world().resource::<CameraSettings>(),
        tweaked,
        "undo never changes how the person's camera feels"
    );

    // The classification itself: two environments differing only in camera
    // feel are the same scene content.
    let a = EnvironmentRecord::default();
    let b = EnvironmentRecord {
        camera: tweaked,
        ..EnvironmentRecord::default()
    };
    assert!(a.scene_content_eq(&b));
    assert_ne!(a, b, "but the camera is still saved");
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

// --- signal-graph edits as undo steps (brief U-2c) -----------------------

/// Applies `edit` to resource `R`, lets it settle, and checks it recorded
/// exactly one `settings` step that undo reverts and redo re-applies.
fn assert_one_settled_step<R>(edit: impl FnOnce(&mut R))
where
    R: Resource<Mutability = bevy::ecs::component::Mutable> + Clone + PartialEq + std::fmt::Debug,
{
    let mut app = paused_app();
    spawn_settled(&mut app); // establish a baseline
    let original = app.world().resource::<R>().clone();
    let before = app.world().resource::<HistoryInfo>().undo_depth;

    edit(&mut app.world_mut().resource_mut::<R>());
    app.update(); // change seen: dirty
    app.update(); // settled: snapshot
    let edited = app.world().resource::<R>().clone();
    assert_ne!(edited, original, "the edit changed the resource");

    let info = *app.world().resource::<HistoryInfo>();
    assert_eq!(info.undo_depth, before + 1, "exactly one step");
    assert_eq!(info.undo_label, Some("settings"));

    undo(&mut app);
    assert_eq!(*app.world().resource::<R>(), original, "undo restores it");
    assert_eq!(body_count(&mut app), 1, "undo stopped at the signal edit");
    redo(&mut app);
    assert_eq!(*app.world().resource::<R>(), edited, "redo re-applies it");
}

#[test]
fn a_signal_binding_edit_is_one_undo_step() {
    assert_one_settled_step::<SignalBindings>(|bindings| {
        bindings.0.push(SignalBinding {
            name: "clock".into(),
            source: SignalSource::Named("t".into()),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::Plot,
        });
    });
}

#[test]
fn a_signal_param_edit_is_one_undo_step() {
    assert_one_settled_step::<SignalParams>(|params| {
        params.0.push(SignalParam::unit("amp"));
    });
}

#[test]
fn a_computed_signal_edit_is_one_undo_step() {
    assert_one_settled_step::<ComputedSignals>(|computed| {
        computed.0.push(ComputedSignal {
            name: "osc".into(),
            expr: SignalExpr::parse_rpn("t sin").unwrap(),
            block: None,
        });
    });
}

/// A param slider drag (a value written every frame) settles into one step.
#[test]
fn a_param_slider_drag_is_one_undo_step() {
    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<SignalParams>()
        .0
        .push(SignalParam::unit("amp"));
    spawn_settled(&mut app); // the spawn's snapshot holds the param
    app.update();
    let before = app.world().resource::<HistoryInfo>().undo_depth;

    for i in 1..=10 {
        app.world_mut().resource_mut::<SignalParams>().0[0].value = 0.5 + i as f32 * 0.04;
        app.update();
    }
    app.update(); // settles
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        before + 1,
        "the whole drag is a single undo step"
    );

    undo(&mut app);
    let value = app.world().resource::<SignalParams>().0[0].value;
    assert!(
        (value - 0.5).abs() < 1e-6,
        "undo restores the pre-drag value"
    );
}
