//! The scripting examples a user sees land on screen (brief 3-6).
//!
//! Spawn verbs take **metres** (1 m = 100 px at start-up zoom), and the
//! start-up view shows about ±6.4 m × ±3.7 m around the origin. Pixel-era
//! numbers (`(spawn-box 0 200 40 40)`) still parse and still spawn, just
//! 200 m up and 40 m wide, so nothing but a test notices when an example
//! drifts back. These run the console's placeholder and every fenced
//! ```` ```scheme ```` example in `docs/scripting.md` through the real
//! script path, then check every authored body against the view box.
//!
//! An example that only registers an action or a tool would spawn nothing
//! here, so registered actions are invoked at the origin and registered
//! tools with points inside the view, as the context menu and the palette
//! would.

use crate::harness::paused_app;
use bevy::prelude::*;
use gradiance::core::script_tools::ScriptTools;
use gradiance::domain::Body;
use gradiance::domain::shape::ShapeDef;
use gradiance::geometry::sdf::aabb;
use gradiance::scene::BodyRecord;
use gradiance::script::bridge::{ScriptActions, ScriptInputs, ScriptLog};

/// Half-extent of the box every example body must sit inside (metres): the
/// start-up view, rounded in from ±6.4 × ±3.7 m.
const VIEW_HALF: Vec2 = Vec2::new(6.0, 4.0);

/// Smallest side a spawned body may have (metres): 5 px at start-up zoom,
/// small enough for a marker dot, large enough to catch a cm-for-m slip.
const MIN_SIDE: f32 = 0.05;

/// Three boxes on screen, so examples that index existing bodies (markers,
/// properties, groups, hinges) have something to act on.
const SEED: &str =
    "(begin (spawn-box -1 0 0.5 0.5) (spawn-box 0 0 0.5 0.5) (spawn-box 1 0 0.5 0.5))";

/// The verb reference block is signatures (`(spawn-box x y w h)`), not a
/// runnable example; it is the only fenced block skipped.
const REFERENCE_MARKER: &str = "(spawn-box x y w h)";

/// Submits `source`, runs the frame it executes in and the one its intents
/// land in, and fails the test if the run errored (a failed run commits
/// nothing, which would make the bounds check pass vacuously).
fn run_ok(app: &mut App, source: &str, what: &str) {
    let before = app.world().resource::<ScriptLog>().0.len();
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
    app.update();
    for entry in &app.world().resource::<ScriptLog>().0[before..] {
        assert!(
            entry.ok,
            "{what} failed: {}\n--- source:\n{}",
            entry.output, entry.input
        );
    }
}

/// Every authored body, as its record.
fn bodies(app: &mut App) -> Vec<BodyRecord> {
    let entities: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<Body>>()
        .iter(app.world())
        .collect();
    entities
        .into_iter()
        .filter_map(|e| BodyRecord::capture(app.world(), e))
        .collect()
}

/// Asserts `body` lies inside the view box at a visible size. A ground
/// half-plane is infinite; only its surface point has to be in view.
fn assert_on_screen(body: &BodyRecord, what: &str) {
    let pos = body.pose.pos;
    let inside = |p: Vec2| p.x.abs() <= VIEW_HALF.x && p.y.abs() <= VIEW_HALF.y;
    if matches!(body.shape, ShapeDef::HalfPlane) {
        assert!(inside(pos), "{what}: ground through {pos} is off screen");
        return;
    }
    let (min, max) = aabb(&body.shape);
    let side = (max - min).min_element();
    assert!(
        side >= MIN_SIDE,
        "{what}: a {:?} body at {pos} is {side} m across (too small to see)",
        body.shape
    );
    let rot = Vec2::from_angle(body.pose.rot);
    for corner in [min, max, Vec2::new(min.x, max.y), Vec2::new(max.x, min.y)] {
        let world = pos + rot.rotate(corner);
        assert!(
            inside(world),
            "{what}: a {:?} body at {pos} reaches {world}, outside ±{VIEW_HALF} m",
            body.shape
        );
    }
}

/// The fenced `scheme` blocks of a markdown document, in order.
fn scheme_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        match current.as_mut() {
            None if line.trim_start() == "```scheme" => current = Some(String::new()),
            None => {}
            Some(_) if line.trim_start() == "```" => blocks.extend(current.take()),
            Some(block) => {
                block.push_str(line);
                block.push('\n');
            }
        }
    }
    blocks
}

#[test]
fn the_console_placeholder_lands_on_screen() {
    let mut app = paused_app();
    run_ok(
        &mut app,
        gradiance::ui::console::PLACEHOLDER,
        "the placeholder",
    );
    let bodies = bodies(&mut app);
    assert_eq!(bodies.len(), 1, "the placeholder spawns one body");
    assert_on_screen(&bodies[0], "the placeholder");
}

/// The verbs whose first argument is a file the run writes.
const WRITING_VERBS: [&str; 2] = ["(save-scene \"", "(signal-export \""];

/// `source` with every file a writing verb names moved under `dir`.
fn redirect_writes(source: &str, dir: &str) -> String {
    let mut out = source.to_owned();
    for verb in WRITING_VERBS {
        out = out.replace(verb, &format!("{verb}{dir}/"));
    }
    out
}

#[test]
fn file_writing_examples_write_into_the_scratch_directory() {
    assert_eq!(
        redirect_writes(
            "(save-scene \"swing.ron\") (signal-export \"s.csv\" (list \"t\"))",
            "/x"
        ),
        "(save-scene \"/x/swing.ron\") (signal-export \"/x/s.csv\" (list \"t\"))"
    );
    let doc = include_str!("../../docs/scripting.md");
    assert!(
        WRITING_VERBS.iter().all(|verb| doc.contains(verb)),
        "the doc still has a file-writing example to redirect"
    );
}

#[test]
fn every_scripting_doc_example_lands_on_screen() {
    let doc = include_str!("../../docs/scripting.md");
    let blocks = scheme_blocks(doc);
    let examples: Vec<&String> = blocks
        .iter()
        .filter(|b| !b.contains(REFERENCE_MARKER))
        .collect();
    assert_eq!(
        blocks.len() - examples.len(),
        1,
        "exactly one block (the verb reference) is skipped"
    );
    // Guards the extractor: the doc has a dozen runnable examples.
    assert!(
        examples.len() >= 10,
        "found only {} examples",
        examples.len()
    );

    // Examples that write files (`save-scene`, `signal-export`) write into a
    // scratch directory, never the working tree (brief R-4: R-1's example
    // rewrote a tracked `swing.ron` at the repository root on every run).
    let scratch =
        std::env::temp_dir().join(format!("gradiance-doc-examples-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap_or_else(|e| panic!("scratch dir: {e}"));
    let scratch_text = scratch.to_string_lossy().replace('\\', "/");

    let mut spawned = 0;
    for (n, example) in examples.iter().enumerate() {
        let what = format!("scripting.md example #{n}");
        let mut app = paused_app();
        run_ok(&mut app, SEED, "the seed scene");
        let seeded = bodies(&mut app).len();
        let example = redirect_writes(example, &scratch_text);
        run_ok(&mut app, &example, &what);

        let actions: Vec<_> = app
            .world()
            .resource::<ScriptActions>()
            .user()
            .cloned()
            .collect();
        for action in &actions {
            app.world_mut()
                .resource_mut::<ScriptInputs>()
                .invoke(action, Vec2::ZERO);
        }
        let tools = app.world().resource::<ScriptTools>().0.clone();
        for tool in &tools {
            // `points` clicks along a short rising diagonal around the origin.
            let points = (0..tool.points)
                .map(|i| Vec2::new(-1.0 + i as f32, 0.5 * i as f32))
                .collect();
            app.world_mut()
                .resource_mut::<ScriptInputs>()
                .invoke_tool(tool, points);
        }
        if !actions.is_empty() || !tools.is_empty() {
            run_ok(&mut app, "0", &format!("{what}'s registered actions/tools"));
        }

        let all = bodies(&mut app);
        spawned += all.len().saturating_sub(seeded);
        for body in &all {
            assert_on_screen(body, &what);
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    // Most examples author bodies; a regression to "nothing spawns" (every
    // example erroring the same way, say) must not pass as "all on screen".
    assert!(spawned >= 10, "the examples spawned only {spawned} bodies");
}
