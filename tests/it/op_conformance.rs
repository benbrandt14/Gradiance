//! Conformance tests for the operation registry (`gradiance-op`, brief U-1;
//! plan `docs/unified-surface-plan.md` § 3.5).
//!
//! Every test iterates the registry, never a hand list, so an operation a
//! layer registers later is covered with no new test. Each one drives the
//! operation from its declared example params (`OpMeta::example`).

use crate::harness::{headless_app_with, step};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::remote::{BrpResult, RemoteMethodSystemId, RemoteMethods};
use gradiance::command::{CommandStack, HistoryInfo};
use gradiance::core::ids::StableId;
use gradiance::domain::Body;
use gradiance::domain::group::SelectionGroup;
use gradiance::interaction::input::KEY_OPS;
use gradiance::interaction::selection::{SelectTransition, SelectedJoint, Selection};
use gradiance::op::{
    InvokeOp, LastOp, LastOpRecord, OpCall, OpEntry, OpIdSource, OpRegistry, OpTrace, Seam,
    TraceEntry,
};
use gradiance::prelude::GameState;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use gradiance::script::remote::{RemoteControlPlugin, method};
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::fmt::Write as _;

/// `(name arg…)` with every argument rendered as a Scheme literal: drives the
/// Lisp doorway from an operation's example. (The remote surface built
/// the same text for VM built-ins until brief U-3b made it registry-only.)
fn call_source(name: &str, args: &[Value]) -> Result<String, String> {
    let mut source = format!("({name}");
    for (i, arg) in args.iter().enumerate() {
        source.push(' ');
        literal(arg, &mut source).map_err(|why| format!("argument {i}: {why}"))?;
    }
    source.push(')');
    Ok(source)
}

/// Appends `value` as a Scheme literal: numbers as numbers, strings quoted
/// and escaped, booleans as `#t`/`#f`, arrays as `(list …)`. `null` and
/// objects have no literal form and are refused.
fn literal(value: &Value, out: &mut String) -> Result<(), &'static str> {
    match value {
        Value::Bool(b) => out.push_str(if *b { "#t" } else { "#f" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(f) = n.as_f64() {
                // `{:?}` keeps a decimal point (`1.0`), so the VM reads a float.
                let _ = write!(out, "{f:?}");
            } else {
                return Err("number out of range");
            }
        }
        Value::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::Array(items) => {
            out.push_str("(list");
            for item in items {
                out.push(' ');
                literal(item, out)?;
            }
            out.push(')');
        }
        Value::Null => return Err("null has no Scheme form"),
        Value::Object(_) => return Err("objects have no Scheme form"),
    }
    Ok(())
}

#[test]
fn args_render_as_scheme_literals() {
    let args = [
        json!(0),
        json!(-2.5),
        json!(1.0),
        json!(true),
        json!("a \"b\"\n"),
        json!([1, "x"]),
    ];
    assert_eq!(
        call_source("f", &args).ok().as_deref(),
        Some(r#"(f 0 -2.5 1.0 #t "a \"b\"\n" (list 1 "x"))"#)
    );
    for bad in [json!(null), json!({"x": 1})] {
        assert!(call_source("f", &[json!(1), bad]).is_err());
    }
}

/// Runs one script whose new ids start at `ids`, and lets it commit.
fn run_with_ids(app: &mut App, ids: u64, source: &str) {
    app.insert_resource(OpIdSource::Sequence(ids));
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    step(app, 2);
}

/// The fixture every operation lowers against, in a paused app with the
/// remote surface on (no transport). Ids are fixed, so index `i` is the
/// `i`-th spawn:
///
/// - bodies 0, 1, 2: circles of radius 0.5 at x = -0.4, 0, 0.4 (y = 1),
///   overlapping pairwise, so every boolean has an effect;
/// - bodies 3, 4: apart at x = 3 and 5, grouped together; body 3 is pinned
///   to the world by joint 0, whose motor speed the binding `drive` names;
/// - one undone spawn on the redo stack, so `redo` has a step to take.
pub(crate) fn fixture() -> App {
    let mut app = headless_app_with(|app| {
        app.add_plugins(RemoteControlPlugin { port: None });
    });
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.update();
    run_with_ids(
        &mut app,
        10,
        "(begin (spawn-circle -0.4 1 0.5) (spawn-circle 0 1 0.5) (spawn-circle 0.4 1 0.5) \
         (spawn-circle 3 1 0.5) (spawn-circle 5 1 0.5))",
    );
    run_with_ids(&mut app, 20, "(begin (hinge 3 -1 3 1) (group (list 3 4)))");
    // A binding, so `signal.unbind`'s example has one to remove.
    run_with_ids(&mut app, 25, "(bind \"drive\" \"t\" \"motor-speed\" 0)");
    run_with_ids(&mut app, 30, "(spawn-circle 0 6 0.5)");
    run_with_ids(&mut app, 40, "(undo)");
    // A saved workspace, so `layout.workspace.delete`'s example has one.
    run_with_ids(&mut app, 50, "(layout-save \"Mine\")");
    app.insert_resource(OpIdSource::Random);
    let history = app.world().resource::<HistoryInfo>();
    assert_eq!(history.redo_depth, 1, "the fixture leaves one step to redo");
    app
}

/// Makes the fixture's newest step adjustable, as if `(spawn-circle 0 6 0.5)`
/// had made it: `edit.adjust_last`'s example edits that call's radius. The
/// fixture cannot have both a step to redo and an operation on top, so the
/// record is planted (U-11).
pub(crate) fn seed_last_op(app: &mut App) {
    let step = app.world().resource::<CommandStack>().revision();
    app.insert_resource(LastOp(Some(LastOpRecord {
        call: OpCall {
            id: "body.spawn_circle",
            params: json!({"x": 0, "y": 6, "r": 0.5}),
            selection: None,
        },
        step,
    })));
}

pub(crate) fn entries(app: &App) -> Vec<OpEntry> {
    app.world().resource::<OpRegistry>().entries().to_vec()
}

pub(crate) fn example(entry: &OpEntry) -> Map<String, Value> {
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

pub(crate) fn depths(app: &App) -> (usize, usize) {
    let history = app.world().resource::<HistoryInfo>();
    (history.undo_depth, history.redo_depth)
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
        seed_last_op(&mut app);

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

/// The sources whose edit rows invoke operations: the UI crate and the key
/// handler.
fn doorway_sources() -> Vec<(std::path::PathBuf, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("crates/gradiance-interaction/src/input.rs")];
    let mut stack = vec![root.join("crates/gradiance-ui/src")];
    while let Some(path) = stack.pop() {
        for item in std::fs::read_dir(&path).unwrap_or_else(|e| panic!("{e}")) {
            let path = item.unwrap_or_else(|e| panic!("{e}")).path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            (path, text)
        })
        .collect()
}

/// Every string literal in `text` shaped like an operation id in one of
/// `namespaces` (`"body.merge"`).
fn id_literals<'t>(text: &'t str, namespaces: &[&str]) -> Vec<&'t str> {
    // Every quote-delimited segment, not only the odd ones: an escaped quote
    // would shift the parity, and a code segment never has an id's shape.
    text.split('"')
        .filter(|lit| {
            lit.split_once('.').is_some_and(|(ns, verb)| {
                namespaces.contains(&ns)
                    && !verb.is_empty()
                    && verb.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            })
        })
        .collect()
}

/// The intent types the registry's edit operations write, from their example
/// outputs: an intent written directly by a menu or key would bypass them.
fn migrated_intents(app: &mut App) -> Vec<String> {
    app.insert_resource(OpTrace {
        dry_run: true,
        entries: Vec::new(),
    });
    let mut names = Vec::new();
    for entry in entries(app) {
        if entry.meta.seam != Seam::Edit {
            continue;
        }
        app.world_mut().write_message(InvokeOp {
            id: Cow::Borrowed(entry.meta.id),
            params: Value::Object(example(&entry)),
        });
        app.update();
        for out in take_trace(app) {
            for write in out.writes {
                let name: String = write
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect();
                if name.ends_with("Intent") && !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

/// Rows that still write a migrated intent directly, with the reason (the
/// brief's Outcome lists them): gesture-style commits that carry old values
/// from one interaction, which no named operation takes yet (U-3).
const DIRECT_INTENT_ROWS: &[(&str, &str)] = &[
    // The node-kind editor, the joint collide checkbox and the inspector
    // sections the menu embeds: inspector fields, U-3's widget registry.
    ("context_menu.rs", "PropertyEditIntent"),
    // Align & distribute: one transform commit computed from bounds.
    ("context_menu.rs", "CommitTransformIntent"),
];

/// Test 4, no stale ids, no side doors: every operation id a UI source or the
/// key handler names is registered, and the menus and the key handler write
/// no intent an operation owns (except [`DIRECT_INTENT_ROWS`]).
#[test]
fn every_invoked_id_is_registered_and_no_side_door_remains() {
    let mut app = fixture();
    let registry = app.world().resource::<OpRegistry>().clone();
    let mut namespaces: Vec<&str> = registry
        .entries()
        .iter()
        .filter_map(|e| e.meta.id.split_once('.').map(|(ns, _)| ns))
        .collect();
    namespaces.dedup();
    let mut found = 0;
    for (path, text) in doorway_sources() {
        for id in id_literals(&text, &namespaces) {
            assert!(
                registry.get(id).is_some(),
                "{}: `{id}` is not a registered operation",
                path.display()
            );
            found += 1;
        }
    }
    assert!(found > 0, "no operation ids found: the scan is broken");

    let intents = migrated_intents(&mut app);
    assert!(
        intents.iter().any(|i| i == "DeleteIntent"),
        "the migrated set is read from the outputs: {intents:?}"
    );
    for (path, text) in doorway_sources() {
        let file = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or_default();
        if !["menu.rs", "context_menu.rs", "input.rs"].contains(&file) {
            continue;
        }
        for intent in &intents {
            let allowed = DIRECT_INTENT_ROWS.contains(&(file, intent.as_str()));
            assert!(
                allowed || !text.contains(intent.as_str()),
                "{file} names `{intent}`: invoke the operation that writes it"
            );
        }
    }
}

/// Points the selection at `bodies` (fixture indices) or at joint 0.
fn select(app: &mut App, bodies: &[usize], joint: bool) {
    let bodies = bodies.to_vec();
    let result = app.world_mut().run_system_once(
        move |mut selection: ResMut<Selection>,
              mut selected_joint: ResMut<SelectedJoint>,
              groups: Query<(Entity, &SelectionGroup), With<Body>>,
              body_ids: Query<(Entity, &StableId), With<Body>>,
              joint_ids: Query<(Entity, &StableId), With<gradiance::domain::Joint>>| {
            let mut sorted: Vec<(Entity, StableId)> =
                body_ids.iter().map(|(e, id)| (e, *id)).collect();
            sorted.sort_by_key(|(_, id)| id.0);
            let picked = bodies.iter().map(|&i| sorted[i].0).collect();
            SelectTransition::SetBodies(picked).apply(&mut selection, &mut selected_joint, &groups);
            if joint && let Some((entity, _)) = joint_ids.iter().next() {
                SelectTransition::SelectJoint(entity).apply(
                    &mut selection,
                    &mut selected_joint,
                    &groups,
                );
            }
        },
    );
    assert!(result.is_ok(), "{result:?}");
}

/// The ids (as text) of the fixture's bodies, in index order, and its joint.
fn fixture_ids(app: &mut App) -> (Vec<String>, String) {
    let world = app.world_mut();
    let mut bodies: Vec<StableId> = world
        .query_filtered::<&StableId, With<Body>>()
        .iter(world)
        .copied()
        .collect();
    bodies.sort_by_key(|id| id.0);
    let joint = world
        .query_filtered::<&StableId, With<gradiance::domain::Joint>>()
        .iter(world)
        .next()
        .map_or_default(|id| id.0.to_string());
    (bodies.iter().map(|id| id.0.to_string()).collect(), joint)
}

/// Test 1b, the key and menu doorway: every row of the key link table names
/// a registered operation. With a fixture selection (bodies 0 and
/// 1, then joint 0), the row's invocation (what the key handler and the menu
/// rows that repeat a key write) and the remote `id` form with the same
/// params lower to the same output, and that output names exactly the
/// selected objects. The Lisp doorway refuses the same params: a script
/// cannot act on the selection.
#[test]
fn every_key_row_acts_on_the_selection_and_a_script_cannot() {
    let mut app = fixture();
    let (bodies, joint) = fixture_ids(&mut app);
    app.insert_resource(OpTrace {
        dry_run: true,
        entries: Vec::new(),
    });
    let registry = app.world().resource::<OpRegistry>().clone();
    assert!(!KEY_OPS.is_empty());
    for row in KEY_OPS {
        let entry = registry
            .get(row.id)
            .unwrap_or_else(|| panic!("{:?}: `{}` is not registered", row.action, row.id));
        let invocation = row.invoke();
        let names_selection = invocation.params.to_string().contains("\"selection\"");
        let mut wrote = false;
        for (picked, selected_joint) in [(&[0_usize, 1][..], false), (&[][..], true)] {
            select(&mut app, picked, selected_joint);
            app.world_mut().write_message(invocation.clone());
            app.update();
            let key = take_trace(&mut app);
            let reply = call(
                &mut app,
                method::OP,
                json!({"id": row.id, "params": invocation.params}),
            );
            assert!(matches!(reply, Some(Ok(_))), "{}: {reply:?}", row.id);
            let remote = take_trace(&mut app);
            assert_eq!(
                key, remote,
                "{}: the key and remote doorways disagree",
                row.id
            );
            let text = key
                .iter()
                .flat_map(|t| t.writes.clone())
                .collect::<String>();
            wrote |= !text.is_empty();
            if names_selection && !text.is_empty() {
                for (i, id) in bodies.iter().enumerate() {
                    let selected = !selected_joint && picked.contains(&i);
                    assert_eq!(
                        text.contains(id.as_str()),
                        selected,
                        "{}: body {i} in `{text}`",
                        row.id
                    );
                }
                assert_eq!(text.contains(joint.as_str()), selected_joint, "{}", row.id);
            }
        }
        assert!(
            wrote,
            "{}: the row wrote nothing for either selection",
            row.id
        );

        if names_selection {
            let params = match &invocation.params {
                Value::Object(fields) => fields.clone(),
                other => panic!("{}: params are not an object: {other:?}", row.id),
            };
            let args: Vec<Value> = entry
                .fields
                .iter()
                .map(|f| params.get(*f).cloned().unwrap_or(Value::Null))
                .collect();
            let source = call_source(entry.meta.lisp, &args)
                .unwrap_or_else(|e| panic!("{}: no Scheme form: {e:?}", row.id));
            select(&mut app, &[0, 1], false);
            app.world_mut()
                .resource_mut::<ScriptInputs>()
                .submit(source.clone());
            app.update();
            let log = app.world().resource::<ScriptLog>().0.last().cloned();
            assert!(
                log.as_ref()
                    .is_some_and(|e| !e.ok && e.output.contains("index or id")),
                "{}: `{source}` must fail visibly: {log:?}",
                row.id
            );
            assert!(
                take_trace(&mut app).is_empty(),
                "{}: a script acted",
                row.id
            );
        }
    }
}

/// Test 6, history: an edit with an undo label records exactly one step with
/// that label; `edit.undo` removes one; a query records nothing; a
/// scene-content config write records one settled step; workstation config
/// and editor state record none.
#[test]
fn every_invocation_changes_history_by_its_declared_step() {
    for entry in entries(&fixture()) {
        let id = entry.meta.id;
        let mut app = fixture();
        seed_last_op(&mut app);
        let (before, redo_before) = depths(&app);
        app.world_mut().write_message(InvokeOp {
            id: Cow::Borrowed(id),
            params: Value::Object(example(&entry)),
        });
        // The settings commit records on the frame after the last change.
        step(&mut app, 3);
        let (after, redo_after) = depths(&app);
        let label = app.world().resource::<HistoryInfo>().undo_label;
        match (entry.meta.seam, entry.meta.undo) {
            (Seam::Query, _) => assert_eq!(after, before, "{id}: a query recorded a step"),
            // Adjust Last Operation replaces the newest step with one of the
            // adjusted operation's: the depth stays and its label shows.
            (Seam::Edit, None) if id == "edit.adjust_last" => {
                assert_eq!(after, before, "{id}: replaces the step in place");
                assert_eq!(label, Some("spawn-body"), "{id}: the operation's step");
            }
            (_, Some(expected)) => {
                assert_eq!(after, before + 1, "{id}: not exactly one step");
                assert_eq!(label, Some(expected), "{id}: the declared undo label");
            }
            (Seam::Edit, None) => {
                // A history operation (undo, redo) moves exactly one step
                // between the two stacks and records none.
                assert_eq!(
                    after.abs_diff(before),
                    1,
                    "{id}: a history operation moves one step"
                );
                assert_eq!(
                    after + redo_after,
                    before + redo_before,
                    "{id}: the step moved between the stacks"
                );
            }
            // Workstation config (`camera.set`) and editor state
            // (`action.register`, …) are never undo steps.
            (Seam::Config | Seam::Editor, None) => {
                assert_eq!(
                    (after, redo_after),
                    (before, redo_before),
                    "{id}: declares no undo step but history moved"
                );
            }
        }
    }
}

/// Where `crates/<crate>/src` files write an intent directly instead of
/// invoking an operation, with the reason each is allowed (brief U-2b). A
/// gesture commits one intent built from what the drag did, and an inspector
/// field carries the old value it shows; neither is a named operation yet
/// (U-3's widget registry). Each row is `(path under crates/, intent, why)`.
const SIDE_DOOR_ALLOW: &[(&str, &str, &str)] = &[
    (
        "gradiance-interaction/src/tools/context.rs",
        "SpawnBodyIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "SpawnNodeIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "CutIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "SpawnJointIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "CommitTransformIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "ScaleIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "DuplicateIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "MergeIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "PropertyEditIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/tools/context.rs",
        "SpawnRodIntent",
        "gesture tools: `ToolCommitWriters::emit`, one commit per gesture",
    ),
    (
        "gradiance-interaction/src/joint_edit.rs",
        "PropertyEditIntent",
        "gesture: a joint anchor or limit handle drag commits one edit on release",
    ),
    (
        "gradiance-interaction/src/node_edit.rs",
        "PropertyEditIntent",
        "gesture: a node drag commits one placement edit (re-attach or free) on release",
    ),
    (
        "gradiance-interaction/src/pack.rs",
        "CommitTransformIntent",
        "gesture: a finished packing run commits one transform of the selection",
    ),
    (
        "gradiance-ui/src/inspector.rs",
        "PropertyEditIntent",
        "inspector fields: each carries the old value it shows (U-3)",
    ),
    (
        "gradiance-ui/src/joint_inspector.rs",
        "PropertyEditIntent",
        "inspector fields: the joint's limits, motor and flags (U-3)",
    ),
    (
        "gradiance-ui/src/depth_panel.rs",
        "PropertyEditIntent",
        "inspector fields and a band drag in the depth plan view (U-3)",
    ),
    (
        "gradiance-ui/src/signals.rs",
        "PropertyEditIntent",
        "inspector fields: a node's signal bindings in the signal list (U-3)",
    ),
    (
        "gradiance-ui/src/bottom_dock.rs",
        "PropertyEditIntent",
        "inspector fields: node edits from the node-graph pane (U-3)",
    ),
    (
        "gradiance-ui/src/context_menu.rs",
        "PropertyEditIntent",
        "inspector fields: the embedded inspector sections, the node-kind editor, \
         the joint collide checkbox (U-3)",
    ),
    (
        "gradiance-ui/src/context_menu.rs",
        "CommitTransformIntent",
        "Align & distribute: one transform commit computed from bounds; no operation fits",
    ),
];

/// Every `(path under crates/, intent)` where `gradiance-ui` or
/// `gradiance-interaction` declares a writer of an intent: a
/// `MessageWriter<…Intent>` or a `Messages<…Intent>` (the resource a direct
/// `write_message` would go through). Comment lines are skipped.
fn intent_writers() -> Vec<(String, String)> {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates");
    let mut found = Vec::new();
    for krate in ["gradiance-ui", "gradiance-interaction"] {
        let mut stack = vec![crates.join(krate).join("src")];
        while let Some(path) = stack.pop() {
            for item in std::fs::read_dir(&path).unwrap_or_else(|e| panic!("{e}")) {
                let path = item.unwrap_or_else(|e| panic!("{e}")).path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let code: String = text
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let rel = path
                    .strip_prefix(&crates)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                for intent in written_intents(&code) {
                    let row = (rel.clone(), intent);
                    if !found.contains(&row) {
                        found.push(row);
                    }
                }
            }
        }
    }
    found
}

/// The intent types `code` declares a writer of.
fn written_intents(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for opener in ["MessageWriter<", "Messages<"] {
        for (at, _) in code.match_indices(opener) {
            // The generic argument, up to its matching `>`.
            let rest = &code[at + opener.len()..];
            let mut depth = 1;
            let end = rest
                .char_indices()
                .find_map(|(i, c)| {
                    match c {
                        '<' => depth += 1,
                        '>' => depth -= 1,
                        _ => {}
                    }
                    (depth == 0).then_some(i)
                })
                .unwrap_or(rest.len());
            // `'w, path::to::Type` → `Type`.
            let ty: String = rest[..end].chars().filter(|c| !c.is_whitespace()).collect();
            let ty = ty.rsplit(',').next().unwrap_or_default();
            let name = ty.rsplit("::").next().unwrap_or_default();
            if name.ends_with("Intent") {
                out.push(name.to_owned());
            }
        }
    }
    out
}

/// Test 3, no side doors (plan § 3.5): `gradiance-ui` and
/// `gradiance-interaction` declare an intent writer only where
/// [`SIDE_DOOR_ALLOW`] lists it with a reason. Every other edit goes through
/// `InvokeOp` and the registry. A row that no longer matches anything is an
/// error too, so the list cannot rot.
#[test]
fn no_intent_writer_outside_the_allow_list() {
    let found = intent_writers();
    assert!(
        found.iter().any(|(_, i)| i == "SpawnBodyIntent"),
        "the scan found no tool writer: it is broken ({found:?})"
    );
    for (path, intent) in &found {
        assert!(
            SIDE_DOOR_ALLOW
                .iter()
                .any(|(p, i, _)| p == path && i == intent),
            "{path} declares a writer of `{intent}`: invoke an operation \
             (`InvokeOp`), or add an allow-list row with the reason"
        );
    }
    for (path, intent, why) in SIDE_DOOR_ALLOW {
        assert!(!why.is_empty(), "{path} {intent}: no reason given");
        assert!(
            found.iter().any(|(p, i)| p == path && i == intent),
            "allow-list row {path} `{intent}` matches nothing: remove it"
        );
    }
}

/// The markdown `docs/ops.md` holds, generated from the registry and the VM
/// built-ins.
fn ops_markdown(registry: &OpRegistry) -> String {
    use gradiance::script::registry::{BUILTINS, OpCategory};
    use std::fmt::Write as _;
    let cell = |text: &str| text.replace('|', "\\|");
    let mut ops: Vec<&OpEntry> = registry.entries().iter().collect();
    ops.sort_by_key(|e| e.meta.id);
    let mut out = String::new();
    let _ = write!(
        out,
        "# Operations\n\n\
         <!-- Generated from the operation registry by `tests/it/op_conformance.rs` \
         (`docs_ops_md_is_current`). Do not edit by hand: run \
         `UPDATE_OPS_DOC=1 cargo test --test it docs_ops_md_is_current`. -->\n\n\
         Every Lisp verb, remote call, menu row and key edit is one of these {} \
         operations, declared once in the layer that owns it (`gradiance-op`; the \
         recipe is in `docs/scripting.md`), or one of the {} VM built-ins at the \
         end. Remote: `gradiance.op {{\"id\", \"params\"}}` takes the id and the \
         named params; `{{\"name\", \"args\"}}` takes the Lisp name and the \
         params in call order.\n\n\
         Seams: **edit** lowers to an undoable intent; **config** writes a \
         settings resource; **query** returns a value and writes nothing; \
         **editor** writes non-authored editor state. A query's NaN (\"nothing \
         there\") is `null` over remote.\n",
        ops.len(),
        BUILTINS.len(),
    );
    let mut namespace = "";
    for entry in ops {
        let meta = entry.meta;
        let ns = meta.id.split_once('.').map_or(meta.id, |(ns, _)| ns);
        if ns != namespace {
            namespace = ns;
            let _ = write!(
                out,
                "\n## `{ns}`\n\n\
                 | Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |\n\
                 |---|---|---|---|---|---|---|\n"
            );
        }
        let surface: Vec<&str> = [meta.menu, meta.chord].into_iter().flatten().collect();
        let _ = writeln!(
            out,
            "| `{}` | `{}` | {} | {} | {} | {} | `{}` |",
            meta.id,
            entry.signature(),
            OpCategory::from(meta.seam).label(),
            meta.undo.map_or("—".to_owned(), |u| format!("`{u}`")),
            if surface.is_empty() {
                "—".to_owned()
            } else {
                cell(&surface.join(", "))
            },
            cell(meta.doc),
            cell(meta.example),
        );
    }
    out.push_str(
        "\n## VM built-ins\n\n\
         These exist only inside a script run and are not registry operations.\n\n\
         | Lisp call | Seam | Doc | Why not an operation |\n\
         |---|---|---|---|\n",
    );
    for builtin in BUILTINS {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            builtin.signature,
            builtin.category.label(),
            cell(builtin.doc),
            cell(builtin.why),
        );
    }
    out
}

/// Test 5, generated docs are current: `docs/ops.md` equals the generator's
/// output for the full app's registry.
#[test]
fn docs_ops_md_is_current() {
    let app = fixture();
    let generated = ops_markdown(app.world().resource::<OpRegistry>());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/ops.md");
    if std::env::var_os("UPDATE_OPS_DOC").is_some() {
        std::fs::write(&path, &generated).unwrap_or_else(|e| panic!("{e}"));
        return;
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    if on_disk != generated {
        let line = on_disk
            .lines()
            .zip(generated.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| on_disk.lines().count().min(generated.lines().count()));
        panic!(
            "docs/ops.md is stale (first difference at line {}): run \
             `UPDATE_OPS_DOC=1 cargo test --test it docs_ops_md_is_current`\n\
             on disk:   {:?}\ngenerated: {:?}",
            line + 1,
            on_disk.lines().nth(line),
            generated.lines().nth(line),
        );
    }
}
