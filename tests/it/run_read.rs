//! The run & read surface (brief R-1): an experiment end to end through
//! operations only. Play, pause and step the simulation and read its clock;
//! dump the scene as JSON; save and load scene files; read and export signal
//! history.

// Test-binary helpers: panics are the failure mechanism.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crate::harness::{paused_app, step};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::op::invoke;
use gradiance::physics::SimStep;
use gradiance::prelude::*;
use gradiance::scene::SceneRecord;
use serde_json::{Value, json};
use std::path::PathBuf;

/// The committed fixture `scene.load`'s example opens.
const PENDULUM: &str = "tests/fixtures/scenes/pendulum.ron";

fn op(app: &mut App, id: &str, params: Value) -> Result<Option<Value>, String> {
    invoke(app.world_mut(), id, params).map_err(|e| e.to_string())
}

fn sim_time(app: &mut App) -> f64 {
    op(app, "sim.time", json!({}))
        .expect("sim.time answers")
        .and_then(|v| v.as_f64())
        .expect("a number")
}

fn state(app: &App) -> GameState {
    *app.world().resource::<State<GameState>>().get()
}

/// Steps frames until no `sim.step` step is owed (at most `limit`).
fn run_owed_steps(app: &mut App, limit: usize) {
    for _ in 0..limit {
        app.update();
        if app.world().resource::<SimStep>().remaining() == 0 {
            app.update();
            return;
        }
    }
    panic!("steps still owed after {limit} frames");
}

/// A fresh directory under the system temp dir for this test.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gradiance-r1-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A hinged pendulum: a ball pinned to the world at (0, 2) by a hinge, held
/// out to the side so it swings.
fn pendulum(app: &mut App) {
    op(app, "body.spawn_circle", json!({"x": 1, "y": 2, "r": 0.2})).expect("spawns");
    step(app, 2);
    op(
        app,
        "joint.hinge",
        json!({"a": 0, "b": -1, "x": 0, "y": 2}),
    )
    .expect("hinges");
    step(app, 2);
}

fn ball_y(app: &mut App) -> f32 {
    let mut query = app
        .world_mut()
        .query_filtered::<&Transform, With<gradiance::domain::Body>>();
    query
        .iter(app.world())
        .next()
        .map(|t| t.translation.y)
        .expect("a body")
}

#[test]
fn play_pause_and_step_drive_the_clock() {
    let mut app = paused_app();
    pendulum(&mut app);

    // Play, then pause: the clock moves only in between.
    op(&mut app, "sim.play", json!({})).expect("plays");
    step(&mut app, 2);
    assert_eq!(state(&app), GameState::Playing);
    step(&mut app, 10);
    op(&mut app, "sim.pause", json!({})).expect("pauses");
    step(&mut app, 2);
    assert_eq!(state(&app), GameState::Paused);
    let t0 = sim_time(&mut app);
    assert!(t0 > 0.0, "the clock ran while playing: {t0}");
    step(&mut app, 10);
    assert!((sim_time(&mut app) - t0).abs() < 1e-9, "paused is frozen");

    // Step 60 at 60 Hz: exactly one simulated second, and it stays paused.
    let y0 = ball_y(&mut app);
    op(&mut app, "sim.step", json!({"n": 60})).expect("steps");
    run_owed_steps(&mut app, 200);
    let t1 = sim_time(&mut app);
    assert!(
        (t1 - t0 - 1.0).abs() < 1e-5,
        "60 steps at 60 Hz are 1 s: {t0} -> {t1}"
    );
    assert_eq!(state(&app), GameState::Paused);
    assert!(ball_y(&mut app) < y0 - 0.05, "the pendulum swung down");
    step(&mut app, 10);
    assert!((sim_time(&mut app) - t1).abs() < 1e-9, "no step past n");

    // A step while playing pauses first; play drops what is still owed.
    op(&mut app, "sim.play", json!({})).expect("plays");
    step(&mut app, 3);
    op(&mut app, "sim.step", json!({"n": 1000})).expect("steps");
    step(&mut app, 3);
    assert_eq!(state(&app), GameState::Paused);
    op(&mut app, "sim.play", json!({})).expect("plays");
    step(&mut app, 2);
    assert_eq!(app.world().resource::<SimStep>().remaining(), 0);
    assert_eq!(state(&app), GameState::Playing);

    // Bad counts are refused before anything is queued.
    assert!(op(&mut app, "sim.step", json!({"n": 0})).is_err());
}

#[test]
fn transport_records_no_undo_step() {
    let mut app = paused_app();
    pendulum(&mut app);
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    op(&mut app, "sim.step", json!({"n": 30})).expect("steps");
    run_owed_steps(&mut app, 100);
    op(&mut app, "sim.play", json!({})).expect("plays");
    step(&mut app, 5);
    assert_eq!(app.world().resource::<HistoryInfo>().undo_depth, depth);
}

#[test]
fn a_dump_survives_save_and_load() {
    let mut app = paused_app();
    pendulum(&mut app);
    op(&mut app, "group.create", json!({"targets": [0]})).expect("groups");
    step(&mut app, 2);
    let before = op(&mut app, "scene.dump", json!({}))
        .expect("dumps")
        .expect("a value");
    // The JSON is the record a save writes: it decodes to what a capture holds.
    let record: SceneRecord = serde_json::from_value(before.clone()).expect("decodes");
    assert_eq!(record, SceneRecord::capture(app.world_mut()));
    assert_eq!(before["joints"].as_array().map(Vec::len), Some(1));
    assert_eq!(before["bodies"].as_array().map(Vec::len), Some(1));

    let dir = temp_dir("dump");
    let path = dir.join("pendulum.ron");
    let text = path.to_string_lossy().into_owned();
    let saved = op(&mut app, "scene.save", json!({"path": text})).expect("saves");
    assert_eq!(saved, Some(Value::from(text.clone())));
    step(&mut app, 1);
    assert!(path.exists(), "the file is written the frame it is asked for");

    op(&mut app, "scene.new", json!({})).expect("clears");
    step(&mut app, 2);
    let empty = op(&mut app, "scene.dump", json!({})).expect("dumps");
    assert_eq!(
        empty.and_then(|d| d["bodies"].as_array().map(Vec::len)),
        Some(0)
    );

    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    op(&mut app, "scene.load", json!({"path": text})).expect("loads");
    step(&mut app, 3);
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        depth + 1,
        "a load is one undo step"
    );
    let after = op(&mut app, "scene.dump", json!({})).expect("dumps");
    assert_eq!(after, Some(before), "dump, save, load, dump: equal");

    // A missing file or a non-RON save path is an error, and nothing moves.
    assert!(op(&mut app, "scene.load", json!({"path": "no/such.ron"})).is_err());
    assert!(op(&mut app, "scene.save", json!({"path": "scene.txt"})).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stepped_signals_record_and_export_as_csv() {
    let mut app = paused_app();
    pendulum(&mut app);
    op(&mut app, "sim.step", json!({"n": 30})).expect("steps");
    run_owed_steps(&mut app, 100);
    op(&mut app, "signal.set", json!({"name": "probe", "value": 2})).expect("publishes");
    step(&mut app, 1);

    let history = op(&mut app, "signal.history", json!({"name": "t"}))
        .expect("reads")
        .expect("a value");
    let samples = history.as_array().expect("a list");
    assert!(samples.len() >= 30, "stepping records: {}", samples.len());
    let times: Vec<f64> = samples
        .iter()
        .map(|s| s[0].as_f64().expect("t"))
        .collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "oldest first");
    // `t` is the clock itself.
    assert!(samples.iter().all(|s| s[0].as_f64() == s[1].as_f64()));
    assert_eq!(
        op(&mut app, "signal.history", json!({"name": "nothing"})),
        Ok(Some(json!([])))
    );

    let dir = temp_dir("csv");
    let path = dir.join("run.csv");
    let text = path.to_string_lossy().into_owned();
    op(
        &mut app,
        "signal.export",
        json!({"path": text, "names": ["t", "probe"]}),
    )
    .expect("exports");
    step(&mut app, 1);
    let csv = std::fs::read_to_string(&path).expect("the file is written");
    let mut lines = csv.lines();
    assert_eq!(lines.next(), Some("t,t,probe"));
    let rows: Vec<Vec<&str>> = lines.map(|l| l.split(',').collect()).collect();
    assert_eq!(rows.len(), samples.len(), "one row per sample of t");
    assert!(rows.iter().all(|r| r.len() == 3));
    for (row, time) in rows.iter().zip(&times) {
        let t: f64 = row[0].parse().expect("t parses");
        assert!((t - time).abs() < 1e-6);
        assert_eq!(row[0], row[1], "t's own column");
    }

    // Unknown names, no names and a non-CSV path are errors.
    for bad in [
        json!({"path": text, "names": ["nothing"]}),
        json!({"path": text, "names": []}),
        json!({"path": "out.txt", "names": ["t"]}),
    ] {
        assert!(op(&mut app, "signal.export", bad).is_err());
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// The committed fixture `scene.load`'s example opens is a current-format
/// pendulum. Regenerate with `UPDATE_FIXTURES=1 cargo test --test it
/// run_read::the_pendulum_fixture_loads`.
#[test]
fn the_pendulum_fixture_loads() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PENDULUM);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        let mut app = paused_app();
        pendulum(&mut app);
        let mut scene = SceneRecord::capture(app.world_mut());
        "fixture".clone_into(&mut scene.app_version);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
        std::fs::write(&path, gradiance::scene::to_ron(&scene).expect("ron")).expect("write");
    }
    let mut app = paused_app();
    let text = path.to_string_lossy().into_owned();
    op(&mut app, "scene.load", json!({"path": text})).expect("loads");
    step(&mut app, 3);
    let dump = op(&mut app, "scene.dump", json!({}))
        .expect("dumps")
        .expect("a value");
    assert_eq!(dump["bodies"].as_array().map(Vec::len), Some(1));
    assert_eq!(dump["joints"].as_array().map(Vec::len), Some(1));
}
