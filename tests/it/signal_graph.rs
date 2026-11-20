//! The signal graph as scene content (brief G-2): built from Lisp through
//! the `signal.*` and `graph.*` operations, saved, loaded and undone
//! exactly, wired by id so a rename keeps its wires, and resolved by name
//! when each edit applies (a script run declares a reader before its param).

// Test-binary helpers: panics are the failure mechanism.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crate::harness::{paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::graph::Input;
use gradiance::op::invoke;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::signal::{Graph, SignalBus};
use serde_json::{Value, json};

/// Runs `source` as one script and settles the frame after (the graph's
/// `settings` step records then).
fn lisp(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    step(app, 3);
    let log = app.world().resource::<ScriptLog>();
    let entry = log.0.last().expect("logged");
    assert!(entry.ok, "`{source}` failed: {}", entry.output);
}

fn dump(app: &mut App) -> Value {
    invoke(app.world_mut(), "scene.dump", json!({}))
        .expect("dumps")
        .expect("a value")
}

fn bus(app: &App, name: &str) -> Option<f32> {
    app.world().resource::<SignalBus>().get(name)
}

fn graph(app: &App) -> &Graph {
    app.world().resource::<Graph>()
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

#[test]
fn a_wired_block_graph_with_positions_saves_loads_and_undoes_exactly() {
    let mut app = paused_app();
    lisp(&mut app, "(defparam \"amp\" 2 0 10)");
    lisp(&mut app, "(graph-add \"signal::gain\" \"boost\")");
    lisp(&mut app, "(graph-connect \"amp\" \"boost\" \"in\")");
    lisp(&mut app, "(graph-set \"boost\" \"k\" 3)");
    lisp(&mut app, "(graph-set \"boost\" \"position\" (list 400 80))");
    assert_eq!(bus(&app, "boost"), Some(6.0), "amp * k through the wire");
    let boost = graph(&app).by_label("boost").expect("the gain").clone();
    assert_eq!(boost.pos, Vec2::new(400.0, 80.0));
    let wired = dump(&mut app);

    let dir = std::env::temp_dir().join(format!("gradiance-g2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graph.ron").to_string_lossy().into_owned();
    invoke(app.world_mut(), "scene.save", json!({"path": path})).expect("saves");
    step(&mut app, 2);

    // A move is one step; undo gives the saved scene back exactly.
    let depth = undo_depth(&app);
    lisp(&mut app, "(graph-set \"boost\" \"position\" (list 10 10))");
    assert_eq!(undo_depth(&app), depth + 1, "one settings step");
    assert_ne!(dump(&mut app), wired);
    undo(&mut app);
    step(&mut app, 2);
    assert_eq!(dump(&mut app), wired, "undo restores the position");

    // Removing the param and loading the file gives the wired graph back.
    lisp(&mut app, "(graph-remove \"amp\")");
    assert_eq!(
        graph(&app).by_label("boost").unwrap().inputs[0],
        Input::External("amp".into()),
        "the wire became a bus read of the removed label"
    );
    invoke(app.world_mut(), "scene.load", json!({"path": path})).expect("loads");
    step(&mut app, 3);
    assert_eq!(dump(&mut app), wired, "load gives the saved graph exactly");
    assert_eq!(bus(&app, "boost"), Some(6.0), "and it computes again");
    let _ = std::fs::remove_dir_all(dir);
}

/// On the by-name model (main before G-2) a rename broke every reader: the
/// reader kept reading the old name, which nothing published any more.
#[test]
fn renaming_a_param_keeps_its_wires() {
    let mut app = paused_app();
    lisp(&mut app, "(defparam \"amp\" 2 0 10)");
    lisp(&mut app, "(defsignal \"osc\" \"amp 3 *\")");
    assert_eq!(bus(&app, "osc"), Some(6.0));

    lisp(&mut app, "(graph-set \"amp\" \"label\" \"gain\")");
    assert_eq!(bus(&app, "gain"), Some(2.0), "the param publishes its new name");
    assert_eq!(bus(&app, "osc"), Some(6.0), "the reader still reads it");
    lisp(&mut app, "(defparam \"gain\" 4 0 10)");
    assert_eq!(bus(&app, "osc"), Some(12.0), "and follows its value");
}

/// Names resolve when each edit applies, not against the run's snapshot: a
/// reader declared before its param in one run is wired to it, and an id a
/// `graph-add` returns can be wired in the same run.
#[test]
fn one_run_wires_a_reader_declared_before_its_param() {
    let mut app = paused_app();
    lisp(
        &mut app,
        "(begin (defsignal \"osc\" \"t sin amp *\") (defparam \"amp\" 2 0 10) \
         (define g (graph-add \"signal::gain\")) (graph-connect \"amp\" g \"in\") \
         (graph-set g \"label\" \"twice\") (graph-set \"twice\" \"k\" 2))",
    );
    let graph = graph(&app);
    let amp = graph.by_label("amp").expect("the param").id;
    let osc = graph.by_label("osc").expect("the reader");
    assert_eq!(osc.inputs[1], Input::wire(amp), "wired by id, not read by name");
    let twice = graph.by_label("twice").expect("the gain, labelled after");
    assert_eq!(twice.inputs[0], Input::wire(amp));
    assert_eq!(bus(&app, "twice"), Some(4.0));
}

#[test]
fn a_refused_graph_edit_changes_nothing() {
    let mut app = paused_app();
    lisp(&mut app, "(defparam \"amp\" 2 0 10)");
    let before = graph(&app).clone();
    let depth = undo_depth(&app);
    // An unknown node, an unknown port, a taken label and a shape node: the
    // run succeeds (names resolve when the edit applies) but the edit is
    // refused there and logged.
    lisp(&mut app, "(graph-connect \"amp\" \"nothing\" \"in\")");
    lisp(&mut app, "(graph-set \"amp\" \"nope\" 1)");
    lisp(
        &mut app,
        "(begin (defsignal \"osc\" \"t\") (graph-set \"osc\" \"label\" \"amp\"))",
    );
    let after = graph(&app);
    assert_eq!(after.by_label("amp"), before.by_label("amp"));
    assert!(after.by_label("osc").is_some(), "the first edit of that run held");
    assert_eq!(undo_depth(&app), depth + 1, "only the defsignal recorded");
    assert!(
        invoke(
            app.world_mut(),
            "graph.add",
            json!({"kind": "shape::box", "label": null})
        )
        .is_err(),
        "shape nodes wait for G-4"
    );
}
