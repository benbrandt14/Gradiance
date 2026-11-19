//! The headless rollout runner, `gradiance run` (brief R-2): the entry the
//! binary calls, driven here with no window and no GPU. Two runs of one
//! falling-box scene give the same CSV byte for byte; a failed load, a script
//! error and an unknown signal are errors with a non-zero exit code.

// Test-binary helpers: panics are the failure mechanism.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crate::harness::{paused_app, step};
use gradiance::op::invoke;
use gradiance::run::{EXIT_FAILED, EXIT_USAGE, RunArgs, RunError, parse_args, rollout};
use gradiance::scene::{SceneRecord, to_ron};
use serde_json::json;
use std::path::{Path, PathBuf};

/// A fresh directory under the system temp dir for this test.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gradiance-r2-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Writes a falling-box scene (a ground, a box that lands on it and a second
/// box that lands on the first, so contacts are solved) and a script that
/// declares the signal `h`, the lower box's height. Returns both paths.
fn falling_boxes(dir: &Path) -> (PathBuf, PathBuf) {
    let mut app = paused_app();
    for (id, params) in [
        ("body.spawn_ground", json!({"x": 0, "y": 0, "angle": 0})),
        ("body.spawn_box", json!({"x": 0, "y": 1.5, "w": 1, "h": 1})),
        (
            "body.spawn_box",
            json!({"x": 0.3, "y": 3, "w": 0.8, "h": 0.6}),
        ),
    ] {
        invoke(app.world_mut(), id, params).expect("spawns");
        step(&mut app, 2);
    }
    let scene = SceneRecord::capture(app.world_mut());
    let lower = scene
        .bodies
        .iter()
        .find(|b| (b.pose.pos.y - 150.0).abs() < 1.0 || (b.pose.pos.y - 1.5).abs() < 0.01)
        .expect("the lower box")
        .id;
    let scene_path = dir.join("boxes.ron");
    std::fs::write(&scene_path, to_ron(&scene).expect("ron")).expect("writes the scene");
    let script_path = dir.join("probe.scm");
    std::fs::write(
        &script_path,
        format!("(defsignal \"h\" \"height@{}\")\n", lower.0),
    )
    .expect("writes the script");
    (scene_path, script_path)
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// The rows of a CSV after its header, as numbers.
fn rows(csv: &str) -> Vec<Vec<f64>> {
    csv.lines()
        .skip(1)
        .map(|l| l.split(',').map(|c| c.parse().expect("a number")).collect())
        .collect()
}

#[test]
fn two_runs_of_a_falling_box_give_the_same_csv() {
    let dir = temp_dir("twice");
    let (scene, script) = falling_boxes(&dir);
    // Longer than the 600-sample plot window: every step is kept.
    let steps = 700_u64;
    let run = |csv: &str| RunArgs {
        scene: scene.clone(),
        scripts: vec![script.clone()],
        steps,
        csv: Some(dir.join(csv)),
        signals: Some(vec!["h".to_owned(), "t".to_owned()]),
    };
    let first = rollout(&run("a.csv")).expect("the first run");
    let second = rollout(&run("b.csv")).expect("the second run");
    let a = std::fs::read(dir.join("a.csv")).expect("a.csv is written");
    let b = std::fs::read(dir.join("b.csv")).expect("b.csv is written");
    assert_eq!(a, first.as_bytes(), "the file holds the returned CSV");
    assert_eq!(a, b, "two runs give the same CSV, byte for byte");
    assert_eq!(first, second);

    assert_eq!(first.lines().next(), Some("t,h,t"));
    let rows = rows(&first);
    assert_eq!(
        rows.len() as u64,
        steps + 1,
        "the state before the first step, then one row per step"
    );
    // One fixed step (1/60 s) per row.
    let dt = 1.0 / 60.0;
    for (i, row) in rows.iter().enumerate() {
        let expected = i as f64 * dt;
        assert!(
            (row[0] - expected).abs() < 1e-3,
            "row {i}: t = {} (expected {expected})",
            row[0]
        );
        assert_eq!(row[0].to_bits(), row[2].to_bits(), "t's own column");
    }
    // The box falls, lands on the ground and rests at half its height.
    let h0 = rows[0][1];
    let rest = rows[rows.len() - 1][1];
    assert!(
        (h0 - 1.5).abs() < 1e-3,
        "starts where the scene put it: {h0}"
    );
    assert!((rest - 0.5).abs() < 0.05, "rests on the ground: {rest}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn without_csv_or_signals_the_run_returns_every_signal() {
    let dir = temp_dir("stdout");
    let (scene, script) = falling_boxes(&dir);
    let parsed = parse_args(&args(&[
        scene.to_str().expect("utf-8"),
        "--script",
        script.to_str().expect("utf-8"),
        "--steps",
        "30",
    ]))
    .expect("parses");
    assert_eq!(parsed.csv, None);
    assert_eq!(parsed.signals, None);
    let csv = rollout(&parsed).expect("runs");
    // Every signal but `t`: here the script's `h` and the sensor it reads.
    let header = csv.lines().next().expect("a header");
    assert!(header.starts_with("t,"), "{header}");
    assert!(header.split(',').any(|c| c == "h"), "{header}");
    assert!(!header.split(',').skip(1).any(|c| c == "t"), "{header}");
    assert_eq!(rows(&csv).len(), 31);

    // A scene with no signals at all exports the clock alone.
    let bare = rollout(&RunArgs {
        scripts: Vec::new(),
        ..parsed
    })
    .expect("runs");
    assert_eq!(bare.lines().next(), Some("t,t"));
    assert_eq!(rows(&bare).len(), 31);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn failures_are_errors_with_a_non_zero_exit_code() {
    let dir = temp_dir("fail");
    let (scene, script) = falling_boxes(&dir);
    let base = RunArgs {
        scene: scene.clone(),
        scripts: vec![script],
        steps: 5,
        csv: None,
        signals: None,
    };

    // A scene that does not read, or does not parse.
    let missing = rollout(&RunArgs {
        scene: dir.join("missing.ron"),
        ..base.clone()
    });
    assert!(matches!(missing, Err(RunError::Scene(_))), "{missing:?}");
    let junk = dir.join("junk.ron");
    std::fs::write(&junk, "(not a scene").expect("writes");
    let unparsed = rollout(&RunArgs {
        scene: junk,
        ..base.clone()
    });
    assert!(matches!(unparsed, Err(RunError::Scene(_))), "{unparsed:?}");

    // A script that errors, and one that does not exist.
    let bad = dir.join("bad.scm");
    std::fs::write(&bad, "(no-such-verb 1 2)").expect("writes");
    let failed = rollout(&RunArgs {
        scripts: vec![bad],
        ..base.clone()
    });
    assert!(matches!(failed, Err(RunError::Script { .. })), "{failed:?}");
    let absent = rollout(&RunArgs {
        scripts: vec![dir.join("absent.scm")],
        ..base.clone()
    });
    assert!(matches!(absent, Err(RunError::Script { .. })), "{absent:?}");

    // A column the bus does not hold.
    let unknown = rollout(&RunArgs {
        signals: Some(vec!["nothing".to_owned()]),
        ..base.clone()
    });
    assert!(matches!(unknown, Err(RunError::Signals(_))), "{unknown:?}");

    for error in [missing, unparsed, failed, absent, unknown] {
        assert_eq!(error.expect_err("an error").exit_code(), EXIT_FAILED);
    }

    // The doorway turns an error into its exit code.
    let scene_arg = scene.to_str().expect("utf-8");
    let missing_arg = dir.join("missing.ron");
    let code = gradiance::run::main(&args(&[
        "run",
        missing_arg.to_str().expect("utf-8"),
        "--steps",
        "1",
    ]));
    assert_eq!(code, Some(EXIT_FAILED));
    let csv = dir.join("out.csv");
    let code = gradiance::run::main(&args(&[
        "run",
        scene_arg,
        "--steps",
        "3",
        "--csv",
        csv.to_str().expect("utf-8"),
    ]));
    assert_eq!(code, Some(0));
    assert!(csv.exists(), "the CSV is written");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_line_parses_and_refuses_misuse() {
    let parsed = parse_args(&args(&[
        "s.ron",
        "--script",
        "a.scm",
        "--script",
        "b.scm",
        "--steps",
        "120",
        "--csv",
        "out.csv",
        "--signals",
        "a, b",
    ]))
    .expect("parses");
    assert_eq!(
        parsed,
        RunArgs {
            scene: PathBuf::from("s.ron"),
            scripts: vec![PathBuf::from("a.scm"), PathBuf::from("b.scm")],
            steps: 120,
            csv: Some(PathBuf::from("out.csv")),
            signals: Some(vec!["a".to_owned(), "b".to_owned()]),
        }
    );
    for bad in [
        &["s.ron"][..],
        &["--steps", "10"],
        &["s.ron", "--steps", "0"],
        &["s.ron", "--steps", "ten"],
        &["s.ron", "--steps"],
        &["s.ron", "t.ron", "--steps", "1"],
        &["s.ron", "--steps", "1", "--csv", "out.txt"],
        &["s.ron", "--steps", "1", "--signals", ","],
        &["s.ron", "--steps", "1", "--frobnicate"],
    ] {
        let error = parse_args(&args(bad)).expect_err("a usage error");
        assert_eq!(error.exit_code(), EXIT_USAGE, "{bad:?}");
    }
    // Only `run` is this doorway's; a bad line is a usage exit.
    assert_eq!(gradiance::run::main(&args(&["ops"])), None);
    assert_eq!(gradiance::run::main(&args(&[])), None);
    assert_eq!(gradiance::run::main(&args(&["run", "--help"])), Some(0));
    assert_eq!(
        gradiance::run::main(&args(&["run", "s.ron"])),
        Some(EXIT_USAGE)
    );
}
