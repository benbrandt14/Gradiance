//! The headless rollout runner, `gradiance run` (brief R-2).
//!
//! ```text
//! gradiance run scene.ron [--script s.scm]… --steps N [--csv out.csv] [--signals a,b]
//! ```
//!
//! Loads a scene, runs the scripts in order, advances exactly `N` fixed
//! physics steps with no window and no GPU, then writes the recorded signals
//! as CSV (to `--csv`, else to stdout) and exits. The deterministic rollout
//! harness behind parameter sweeps (a shell loop over `run`), system
//! identification and replay tests.
//!
//! Unlike the client doorways (`ops`, `op`, `mcp`, which talk to a running
//! app), `run` **is** a host: it owns the clock. It is a composition of the
//! R-1 operations, not new logic: `scene.load` opens the file, the scripts go
//! through the ordinary script doorway, `sim.step` owes the steps, and the
//! CSV is [`signals_csv`] (what `signal.export` writes). The two things it
//! adds are the clock (one update = one fixed tick) and a lifted signal
//! history cap, so a rollout longer than the plot window keeps every row.
//!
//! Exit codes: `0` done, `1` a failed load, a script error, an unknown
//! signal or a failed write, `2` a usage error.

use crate::GradiancePlugins;
use bevy::app::{TaskPoolOptions, TaskPoolPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use gradiance_core::states::GameState;
use gradiance_op::{OpContext, invoke};
use gradiance_physics::SimStep;
use gradiance_script::bridge::ScriptInputs;
use gradiance_script::ops::{SignalHistories, signals_csv};
use gradiance_signal::{SignalBus, TIME_INPUT};
use serde_json::json;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The subcommand word.
pub const SUBCOMMAND: &str = "run";

/// Exit code of a run that failed (load, script, signal, write).
pub const EXIT_FAILED: i32 = 1;

/// Exit code of a usage error.
pub const EXIT_USAGE: i32 = 2;

/// The most steps one `sim.step` request owes; a longer rollout is several.
const STEP_CHUNK: u64 = 216_000;

/// Frames a script gets to report its outcome before the run gives up.
const SCRIPT_FRAMES: usize = 8;

/// Frames for a load to land and its derived state (colliders) to settle.
const SETTLE_FRAMES: usize = 3;

/// The usage text.
pub const USAGE: &str = "\
usage: gradiance run <scene.ron> [--script <file.scm>]... --steps <N>
                     [--csv <out.csv>] [--signals <a,b,...>]

Loads the scene, runs the scripts in order, advances exactly N fixed physics
steps with no window, and writes the recorded signals as CSV: a header
t,<signals>, then one row per recorded sample. The rows go to --csv, else to
stdout. --signals names the columns (default: every signal on the bus but t;
just t when there is none). Steps a script asks for with (sim-step k) run
before the N.

exit codes: 0 done, 1 failed (load, script, signal, write), 2 usage";

/// A parsed `gradiance run` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunArgs {
    /// The scene file to load (`.ron`).
    pub scene: PathBuf,
    /// Scripts to run after the load, in order.
    pub scripts: Vec<PathBuf>,
    /// Fixed physics steps to advance after the scripts.
    pub steps: u64,
    /// Where the CSV goes (`.csv`); `None` writes it to stdout.
    pub csv: Option<PathBuf>,
    /// The CSV's signal columns; `None` takes every signal on the bus.
    pub signals: Option<Vec<String>>,
}

/// Why a run stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The command line is wrong.
    Usage(String),
    /// The scene did not read or parse.
    Scene(String),
    /// A script did not read, or its run failed.
    Script {
        /// The script file.
        path: PathBuf,
        /// The error text (the console's).
        message: String,
    },
    /// `sim.step` refused the request.
    Step(String),
    /// The steps did not complete (the app stopped advancing).
    Stalled {
        /// Steps still owed.
        remaining: u32,
    },
    /// A `--signals` column the bus does not hold, or a bad name.
    Signals(String),
    /// The CSV could not be written.
    Write {
        /// The CSV file.
        path: PathBuf,
        /// The IO error text.
        message: String,
    },
}

impl RunError {
    /// The process exit code for this error.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => EXIT_USAGE,
            _ => EXIT_FAILED,
        }
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(why) => write!(f, "{why}"),
            Self::Scene(why) => write!(f, "scene: {why}"),
            Self::Script { path, message } | Self::Write { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            Self::Step(why) => write!(f, "sim.step: {why}"),
            Self::Stalled { remaining } => {
                write!(f, "the simulation stopped with {remaining} steps owed")
            }
            Self::Signals(why) => write!(f, "signals: {why}"),
        }
    }
}

impl std::error::Error for RunError {}

/// The `run` doorway of the binary: `None` unless `args` (the process
/// arguments after the program name) start with `run`; otherwise runs it and
/// returns the exit code. Errors go to stderr, the CSV to its file or stdout.
pub fn main(args: &[String]) -> Option<i32> {
    if args.first().map(String::as_str) != Some(SUBCOMMAND) {
        return None;
    }
    let rest = &args[1..];
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return Some(0);
    }
    let result = parse_args(rest).and_then(|run| rollout(&run).map(|csv| (run, csv)));
    Some(match result {
        Ok((run, csv)) => {
            if run.csv.is_none() {
                print!("{csv}");
            }
            0
        }
        Err(e) => {
            eprintln!("gradiance run: {e}");
            if matches!(e, RunError::Usage(_)) {
                eprintln!("{USAGE}");
            }
            e.exit_code()
        }
    })
}

/// Parses the arguments after `run`.
pub fn parse_args(args: &[String]) -> Result<RunArgs, RunError> {
    let usage = |why: String| Err(RunError::Usage(why));
    let mut scene = None;
    let mut scripts = Vec::new();
    let mut steps = None;
    let mut csv = None;
    let mut signals = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| RunError::Usage(format!("{flag} takes a value")))
        };
        match arg.as_str() {
            "--script" => scripts.push(PathBuf::from(value("--script")?)),
            "--steps" => {
                let text = value("--steps")?;
                match text.parse::<u64>() {
                    Ok(n) if n > 0 => steps = Some(n),
                    _ => return usage(format!("--steps is a whole number above 0: `{text}`")),
                }
            }
            "--csv" => {
                let path = PathBuf::from(value("--csv")?);
                if !has_extension(&path, "csv") {
                    return usage(format!("--csv names a .csv file: `{}`", path.display()));
                }
                csv = Some(path);
            }
            "--signals" => {
                let names: Vec<String> = value("--signals")?
                    .split(',')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
                    .collect();
                if names.is_empty() {
                    return usage("--signals names at least one signal".to_owned());
                }
                signals = Some(names);
            }
            flag if flag.starts_with("--") => return usage(format!("unknown flag `{flag}`")),
            path => {
                if scene.is_some() {
                    return usage(format!("one scene per run (a second: `{path}`)"));
                }
                scene = Some(PathBuf::from(path));
            }
        }
    }
    let Some(scene) = scene else {
        return usage("name a scene file".to_owned());
    };
    let Some(steps) = steps else {
        return usage("--steps is required".to_owned());
    };
    Ok(RunArgs {
        scene,
        scripts,
        steps,
        csv,
        signals,
    })
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// The frame duration the test harness and the start of a run use (60 fps).
pub const FRAME: Duration = Duration::from_micros(16_667);

/// A windowless, GPU-less app with the full Gradiance stack and a manual
/// clock that advances `FRAME` per update; `setup` runs before start-up
/// (where the binary inserts its CLI resources). The integration suite's
/// harness and [`rollout`] share it. Nothing here opens a window: winit,
/// rendering and egui's output are absent (`MinimalPlugins`), and the render
/// and UI plugins' systems find no window or GPU to act on.
pub fn headless_app_with(setup: impl FnOnce(&mut App)) -> App {
    build_headless(None, setup)
}

fn build_headless(threads: Option<usize>, setup: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    let minimal = MinimalPlugins.build();
    let minimal = match threads {
        Some(n) => minimal.set(TaskPoolPlugin {
            task_pool_options: TaskPoolOptions::with_num_threads(n),
        }),
        None => minimal,
    };
    app.add_plugins((
        minimal,
        StatesPlugin,
        TransformPlugin,
        bevy::input::InputPlugin,
    ));
    app.add_plugins(GradiancePlugins);
    // Advance exactly one frame of virtual time per update, regardless of
    // wall-clock speed, so physics is reproducible.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    setup(&mut app);
    // Without a runner, plugin `finish`/`cleanup` never run — avian
    // registers diagnostics resources there, so run them explicitly.
    app.finish();
    app.cleanup();
    // Flush startup so states and resources are initialized.
    app.update();
    app
}

/// Runs `run` headless and returns the CSV text; with `--csv` the text is
/// also written to that file. Every step is one fixed tick of one update, on
/// one thread, so two runs of the same inputs give the same CSV.
pub fn rollout(run: &RunArgs) -> Result<String, RunError> {
    // Read every script first: a missing file fails before the app is built.
    let mut sources = Vec::with_capacity(run.scripts.len());
    for path in &run.scripts {
        let source = std::fs::read_to_string(path).map_err(|e| RunError::Script {
            path: path.clone(),
            message: e.to_string(),
        })?;
        sources.push((path.clone(), source));
    }

    // One compute thread: the run's order of work is the same every time,
    // whatever the machine (avian is built with `parallel`).
    // Paused from the first frame: nothing moves or records before the steps.
    let mut app = build_headless(Some(1), |app| {
        app.insert_state(GameState::Paused);
    });
    // Every sample of the rollout is kept; recording is bounded by the steps.
    app.world_mut()
        .resource_mut::<SignalBus>()
        .set_history_cap(usize::MAX);

    let scene = run.scene.to_string_lossy().into_owned();
    invoke(app.world_mut(), "scene.load", json!({ "path": scene }))
        .map_err(|e| RunError::Scene(e.to_string()))?;
    settle(&mut app);

    for (path, source) in sources {
        let reply = app
            .world_mut()
            .resource_mut::<ScriptInputs>()
            .submit_with_reply(source);
        let mut outcome = None;
        for _ in 0..SCRIPT_FRAMES {
            app.update();
            outcome = reply.lock().ok().and_then(|mut slot| slot.take());
            if outcome.is_some() {
                break;
            }
        }
        match outcome {
            Some(Ok(_)) => {}
            Some(Err(message)) => return Err(RunError::Script { path, message }),
            None => {
                return Err(RunError::Script {
                    path,
                    message: "the script did not run".to_owned(),
                });
            }
        }
        settle(&mut app);
    }
    // Steps a script asked for run before the run's own.
    finish_owed_steps(&mut app, 0)?;

    let mut left = run.steps;
    while left > 0 {
        let chunk = left.min(STEP_CHUNK);
        invoke(app.world_mut(), "sim.step", json!({ "n": chunk }))
            .map_err(|e| RunError::Step(e.to_string()))?;
        finish_owed_steps(&mut app, chunk)?;
        left -= chunk;
    }

    let csv = signals_csv_of(&mut app, run.signals.as_deref())?;
    if let Some(path) = &run.csv {
        std::fs::write(path, &csv).map_err(|e| RunError::Write {
            path: path.clone(),
            message: e.to_string(),
        })?;
    }
    Ok(csv)
}

/// Runs a few frames so a load or a script's edits land and their derived
/// state is rebuilt (paused: the clock does not move).
fn settle(app: &mut App) {
    for _ in 0..SETTLE_FRAMES {
        app.update();
    }
}

/// Updates until no `sim.step` step is owed. Each update advances virtual
/// time by exactly the fixed timestep, so each owes one fixed tick and the
/// signals record once per step. A run that stops advancing is an error,
/// never a hang.
fn finish_owed_steps(app: &mut App, expected: u64) -> Result<(), RunError> {
    // The request lands in the first update; then one step per update.
    let limit = expected.saturating_mul(2).saturating_add(16);
    let mut frames = 0_u64;
    loop {
        let timestep = app.world().resource::<Time<Fixed>>().timestep();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(timestep));
        app.update();
        frames += 1;
        let remaining = app.world().resource::<SimStep>().remaining();
        if remaining == 0 {
            return Ok(());
        }
        if frames > limit {
            return Err(RunError::Stalled { remaining });
        }
    }
}

/// The CSV of the recorded signals: `names`, else every signal on the bus but
/// `t` (just `t` when the bus holds nothing else).
fn signals_csv_of(app: &mut App, names: Option<&[String]>) -> Result<String, RunError> {
    let ctx = OpContext::build(app.world_mut());
    let histories = ctx
        .get::<SignalHistories>()
        .map_err(|e| RunError::Signals(e.to_string()))?;
    let names: Vec<String> = if let Some(names) = names {
        names.to_vec()
    } else {
        let bus = app.world().resource::<SignalBus>();
        let mut all: Vec<String> = bus
            .entries()
            .map(|(name, _)| name)
            .filter(|name| *name != TIME_INPUT)
            .map(str::to_owned)
            .collect();
        if all.is_empty() {
            all.push(TIME_INPUT.to_owned());
        }
        all
    };
    signals_csv(histories, &names).map_err(|e| RunError::Signals(e.to_string()))
}
