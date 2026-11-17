//! Conformance tests for the operation registry (`gradiance-op`, brief U-1;
//! plan `docs/unified-surface-plan.md` § 3.5).
//!
//! Every test iterates the registry, never a hand list, so an operation a
//! layer registers later is covered with no new test. Each one drives the
//! operation from its declared example params (`OpMeta::example`).

use crate::harness::{headless_app_with, step};
use bevy::prelude::*;
use bevy::remote::{BrpResult, RemoteMethodSystemId, RemoteMethods};
use gradiance::command::HistoryInfo;
use gradiance::op::{InvokeOp, OpEntry, OpIdSource, OpRegistry, OpTrace, Seam, TraceEntry};
use gradiance::prelude::GameState;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::script::remote::{RemoteControlPlugin, call_source, method};
use serde_json::{Map, Value, json};
use std::borrow::Cow;

/// Three bodies, spawned and committed, in a paused app with the remote
/// surface on (no transport). The fixture every operation lowers against.
fn fixture() -> App {
    let mut app = headless_app_with(|app| {
        app.add_plugins(RemoteControlPlugin { port: None });
    });
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.update();
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit("(begin (spawn-circle -2 1 0.5) (spawn-circle 0 1 0.5) (spawn-circle 2 1 0.5))");
    step(&mut app, 2);
    app
}

fn entries(app: &App) -> Vec<OpEntry> {
    app.world().resource::<OpRegistry>().entries().to_vec()
}

fn example(entry: &OpEntry) -> Map<String, Value> {
    match serde_json::from_str(entry.meta.example) {
        Ok(Value::Object(fields)) => fields,
        other => panic!("{}: example is not a JSON object: {other:?}", entry.meta.id),
    }
}

/// The example's values in declared field order (the positional form).
fn positional(entry: &OpEntry) -> Vec<Value> {
    let fields = example(entry);
    entry
        .fields
        .iter()
        .map(|name| fields.get(*name).cloned().unwrap_or(Value::Null))
        .collect()
}

/// Calls one remote method as the BRP server would.
fn call(app: &mut App, name: &str, params: Value) -> Option<BrpResult> {
    let id = match app.world().resource::<RemoteMethods>().get(name)? {
        RemoteMethodSystemId::Instant(id) => *id,
        RemoteMethodSystemId::Watching(_) => return None,
    };
    app.world_mut().run_system_with(id, Some(params)).ok()
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

/// The outputs the registry applied (or withheld, in a dry run) since the
/// trace was cleared.
fn take_trace(app: &mut App) -> Vec<TraceEntry> {
    std::mem::take(&mut app.world_mut().resource_mut::<OpTrace>().entries)
}

/// Test 1, doorway equivalence: the Lisp verb, the remote `id` form and the
/// UI invoke path lower one call to the same output (writes and value),
/// against one scene and one id sequence. A dry run withholds every write,
/// so all three see the same scene.
#[test]
fn every_doorway_lowers_a_call_to_the_same_output() {
    let mut app = fixture();
    app.insert_resource(OpIdSource::Sequence(1000));
    app.insert_resource(OpTrace {
        dry_run: true,
        entries: Vec::new(),
    });
    for entry in entries(&app) {
        let id = entry.meta.id;
        let params = Value::Object(example(&entry));

        // Lisp: the generated verb, positional args in declared order.
        let source = call_source(entry.meta.lisp, &positional(&entry))
            .unwrap_or_else(|e| panic!("{id}: example has no Scheme form: {e:?}"));
        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit(source.clone());
        app.update();
        let log = app.world().resource::<ScriptLog>().0.last().cloned();
        assert!(
            log.as_ref().is_some_and(|e| e.ok),
            "{id}: `{source}` failed: {log:?}"
        );
        let lisp = take_trace(&mut app);

        // Remote: named params, the registry's serde decoder.
        let reply = call(&mut app, method::OP, json!({"id": id, "params": params}));
        assert!(
            matches!(reply, Some(Ok(_))),
            "{id}: remote call failed: {reply:?}"
        );
        let remote = take_trace(&mut app);

        // UI: an invoke message, as a menu row writes it.
        app.world_mut().write_message(InvokeOp {
            id: Cow::Borrowed(id),
            params: params.clone(),
        });
        app.update();
        let ui = take_trace(&mut app);

        assert_eq!(lisp.len(), 1, "{id}: Lisp applied {lisp:?}");
        assert_eq!(lisp, remote, "{id}: Lisp and remote disagree");
        assert_eq!(lisp, ui, "{id}: Lisp and the UI path disagree");
        let out = &lisp[0];
        assert_eq!(out.id, id);
        // The example must exercise the operation, or equality proves little.
        match entry.meta.seam {
            Seam::Query => assert!(out.value.is_some(), "{id}: a query returned nothing"),
            _ => assert!(!out.writes.is_empty(), "{id}: the example writes nothing"),
        }
    }
}

/// Test 2, schema round trip: the declared fields (reflection) and the
/// example (what the serde decoder reads) name the same keys, and the
/// positional mapping rebuilds the example exactly.
#[test]
fn declared_fields_and_the_example_agree() {
    let app = fixture();
    for entry in entries(&app) {
        let named = example(&entry);
        let mut keys: Vec<&str> = named.keys().map(String::as_str).collect();
        let mut fields = entry.fields.clone();
        keys.sort_unstable();
        fields.sort_unstable();
        assert_eq!(
            keys, fields,
            "{}: example keys vs declared fields",
            entry.meta.id
        );
        let rebuilt = entry.named(positional(&entry)).ok();
        assert_eq!(rebuilt, Some(Value::Object(named)), "{}", entry.meta.id);
        assert!(
            entry
                .signature()
                .starts_with(&format!("({}", entry.meta.lisp)),
            "{}",
            entry.meta.id
        );
    }
}

/// How a UI source invokes an operation by id.
const INVOKE: &str = "InvokeOp::new(\"";

/// Test 4, no stale ids: every `InvokeOp::new("…")` a UI source writes names a
/// registered operation, and a migrated menu row writes no intent of its own.
#[test]
fn every_invoked_id_in_the_ui_is_registered() {
    let app = fixture();
    let registry = app.world().resource::<OpRegistry>();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/gradiance-ui/src");
    let mut found = 0;
    let mut stack = vec![dir];
    while let Some(path) = stack.pop() {
        for item in std::fs::read_dir(&path).unwrap_or_else(|e| panic!("{e}")) {
            let path = item.unwrap_or_else(|e| panic!("{e}")).path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (at, pattern) in text.match_indices(INVOKE) {
                let id = text[at + pattern.len()..]
                    .split('"')
                    .next()
                    .unwrap_or_default();
                assert!(
                    registry.get(id).is_some(),
                    "{}: `{id}` is not a registered operation",
                    path.display()
                );
                found += 1;
            }
        }
    }
    assert!(found > 0, "no InvokeOp::new rows found: the scan is broken");
    let menu = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/gradiance-ui/src/menu.rs"),
    )
    .unwrap_or_default();
    assert!(
        !menu.contains("UndoIntent"),
        "the Undo row is migrated: it invokes `edit.undo`, it writes no intent"
    );
}

/// Test 6, history: an edit with an undo label records exactly one step with
/// that label; `edit.undo` removes one; a query records nothing; a config
/// write records one settled step.
#[test]
fn every_invocation_changes_history_by_its_declared_step() {
    for entry in entries(&fixture()) {
        let id = entry.meta.id;
        let mut app = fixture();
        let before = undo_depth(&app);
        app.world_mut().write_message(InvokeOp {
            id: Cow::Borrowed(id),
            params: Value::Object(example(&entry)),
        });
        // The settings commit records on the frame after the last change.
        step(&mut app, 3);
        let after = undo_depth(&app);
        let label = app.world().resource::<HistoryInfo>().undo_label;
        match (entry.meta.seam, entry.meta.undo) {
            (Seam::Query, _) => assert_eq!(after, before, "{id}: a query recorded a step"),
            (_, Some(expected)) => {
                assert_eq!(after, before + 1, "{id}: not exactly one step");
                assert_eq!(label, Some(expected), "{id}: the declared undo label");
            }
            (Seam::Edit, None) => {
                assert_eq!(
                    after + 1,
                    before,
                    "{id}: a history operation removes one step"
                );
            }
            (seam, None) => panic!("{id}: a {seam:?} operation must declare its undo label"),
        }
    }
}
