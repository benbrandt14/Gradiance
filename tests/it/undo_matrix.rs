//! The undo round-trip matrix (brief 6-1): every operation that records or
//! moves a history step, driven from its declared example on the
//! conformance fixture (`op_conformance::fixture`), must give the scene back
//! exactly on undo and the edited scene back exactly on redo.
//!
//! The test iterates the registry, never a hand list, so an operation a
//! layer registers later is covered with no new test. "The scene" is what
//! `scene.dump` returns (the record a save writes, as JSON), minus the
//! fields in [`NOT_UNDONE`].

use crate::harness::step;
use crate::op_conformance::{depths, entries, example, fixture, seed_last_op};
use bevy::prelude::*;
use gradiance::op::{OpEntry, Seam, invoke};
use serde_json::{Value, json};

/// Environment fields left out of the compare, each with its reason. They
/// are **workstation config** (CLAUDE.md invariant 4, the `scene_content!`
/// list in `scene::records`): a load writes them, as a save writes them, but
/// undo never reverts them by design, so reverting an edit cannot move the
/// person's grid, snap, camera feel or saved dock layouts.
pub(crate) const NOT_UNDONE: &[(&str, &str)] = &[
    ("grid", "workstation config: saved, never undone"),
    ("snap", "workstation config: saved, never undone"),
    ("camera", "workstation config: saved, never undone"),
    ("workspaces", "workstation config: saved, never undone"),
];

/// Operations whose example is the identity on the fixture, with the reason.
/// Their round trip is still checked (the step is recorded, undone and
/// redone); only the "the call changed the scene" check is waived. A row
/// whose example does change the scene fails the test, so the list cannot rot.
const IDENTITY_ON_FIXTURE: &[(&str, &str)] = &[(
    "body.mirror_v",
    "every fixture body is a circle on y = 1, so a vertical mirror about the \
     bounds centre maps the scene onto itself",
)];

/// The scene as `scene.dump` returns it, minus [`NOT_UNDONE`].
fn dump(app: &mut App) -> Value {
    let mut value = invoke(app.world_mut(), "scene.dump", json!({}))
        .unwrap_or_else(|e| panic!("scene.dump failed: {e}"))
        .unwrap_or_else(|| panic!("scene.dump returned nothing"));
    if let Some(env) = value.get_mut("environment").and_then(Value::as_object_mut) {
        for (field, _) in NOT_UNDONE {
            assert!(
                env.remove(*field).is_some(),
                "`{field}` is not in the dump: update NOT_UNDONE"
            );
        }
    }
    value
}

/// Whether `entry` records or moves a history step: every `edit` operation
/// (undo and redo included) and every scene-content `config` write (one
/// settled `settings` step).
fn in_matrix(entry: &OpEntry) -> bool {
    entry.meta.seam == Seam::Edit || entry.meta.undo.is_some()
}

/// Invokes `id` with `params`, then steps until history moves (an
/// operation like `pack.run` commits over several frames; a settings write
/// records on the frame after the last change), at most `limit` frames.
fn run(app: &mut App, id: &str, params: Value) {
    let before = (
        depths(app),
        app.world()
            .resource::<gradiance::command::CommandStack>()
            .revision(),
    );
    if let Err(e) = invoke(app.world_mut(), id, params) {
        panic!("{id}: the call failed: {e}");
    }
    step(app, 3);
    let limit = 600;
    for _ in 0..limit {
        let now = (
            depths(app),
            app.world()
                .resource::<gradiance::command::CommandStack>()
                .revision(),
        );
        if now != before {
            break;
        }
        app.update();
    }
}

/// First differing JSON path between two dumps, for a readable failure.
fn first_difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                let (l, r) = (x.get(key), y.get(key));
                if l != r {
                    return match (l, r) {
                        (Some(l), Some(r)) => first_difference(l, r, &format!("{path}.{key}")),
                        _ => Some(format!("{path}.{key}: {l:?} vs {r:?}")),
                    };
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (l, r))| first_difference(l, r, &format!("{path}[{i}]"))),
        _ if a == b => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

fn assert_same(id: &str, what: &str, got: &Value, want: &Value) {
    if let Some(at) = first_difference(got, want, "") {
        panic!("{id}: {what} differs at {at}");
    }
}

/// The matrix: for every operation that records or moves a history step,
/// from its example on the fixture: dump, invoke, dump, undo, dump, redo,
/// dump. Undo gives the first dump back exactly, redo the second, and the
/// history depths move by the declared step at each stage.
///
/// Two operations invert differently, by what they are: `edit.undo` is
/// reversed by a redo (and repeated by an undo), and `edit.adjust_last`
/// replaces the newest step, so its undo returns to the scene *before
/// the replaced step* (the fixture after one undo), not the pre-call scene.
#[test]
fn every_history_operation_round_trips_through_undo_and_redo() {
    let registry = entries(&fixture());
    let mut covered = 0;
    for entry in registry.iter().filter(|e| in_matrix(e)) {
        let id = entry.meta.id;
        let params = Value::Object(example(entry));
        let mut app = fixture();
        seed_last_op(&mut app);
        let (u0, r0) = depths(&app);
        let d0 = dump(&mut app);
        // What the inverse step gives back.
        let expect_inverse = if id == "edit.adjust_last" {
            let mut before_step = fixture();
            run(&mut before_step, "edit.undo", json!({}));
            dump(&mut before_step)
        } else {
            d0.clone()
        };

        run(&mut app, id, params);
        let (u1, r1) = depths(&app);
        let d1 = dump(&mut app);
        let (inverse, again) = if id == "edit.undo" {
            ("edit.redo", "edit.undo")
        } else {
            ("edit.undo", "edit.redo")
        };
        // The declared step: a recorded edit is one step on the undo stack
        // and clears the redo branch; a history operation moves one step
        // between the stacks; Adjust Last replaces the newest step.
        let expected = match (id, entry.meta.undo) {
            ("edit.adjust_last", _) => (u0, 0),
            ("edit.undo", _) => (u0 - 1, r0 + 1),
            ("edit.redo", _) => (u0 + 1, r0 - 1),
            (_, Some(_)) => (u0 + 1, 0),
            (_, None) => panic!("{id}: an edit with no undo step and no history rule"),
        };
        assert_eq!((u1, r1), expected, "{id}: depths after the call");
        // The example must change the scene, or the round trip proves little.
        let identity = IDENTITY_ON_FIXTURE.iter().any(|(op, _)| *op == id);
        assert_eq!(
            d1 == d0,
            identity,
            "{id}: the example changed the scene: {}; IDENTITY_ON_FIXTURE lists it only when it does not",
            first_difference(&d1, &d0, "").unwrap_or_default()
        );

        run(&mut app, inverse, json!({}));
        // The inverse moves the one step back between the stacks.
        let back = if id == "edit.undo" {
            (u1 + 1, r1 - 1)
        } else {
            (u1 - 1, r1 + 1)
        };
        assert_eq!(depths(&app), back, "{id}: depths after {inverse}");
        assert_same(
            id,
            &format!("the scene after {inverse}"),
            &dump(&mut app),
            &expect_inverse,
        );

        run(&mut app, again, json!({}));
        assert_eq!(depths(&app), (u1, r1), "{id}: {again} restores the depths");
        assert_same(
            id,
            &format!("the scene after {again}"),
            &dump(&mut app),
            &d1,
        );
        covered += 1;
    }
    assert!(
        covered >= 40,
        "the matrix covered only {covered} operations"
    );
}
