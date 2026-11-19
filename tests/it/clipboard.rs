//! Insert a scene file and copy/paste bodies as RON text (brief R-3).
//!
//! Insert and paste are one undo step each, keep the joints inside the set
//! and the nested groups (with their names), and never reuse a `StableId`
//! the scene holds. Ctrl+C / Ctrl+V on the canvas go through the same
//! operations; a number field under the pointer keeps both keys.

// Test-binary helpers: panics are the failure mechanism.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crate::harness::{paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::command::intent::NameGroupIntent;
use gradiance::interaction::cursor::CursorWorldPos;
use gradiance::interaction::{FieldKeys, KeyboardCaptured};
use gradiance::op::invoke;
use gradiance::prelude::*;
use gradiance::scene::{SceneRecord, from_ron};
use gradiance::ui::clipboard::LastCopy;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

fn op(app: &mut App, id: &str, params: Value) -> Result<Option<Value>, String> {
    invoke(app.world_mut(), id, params).map_err(|e| e.to_string())
}

fn capture(app: &mut App) -> SceneRecord {
    SceneRecord::capture(app.world_mut())
}

fn history(app: &App) -> (usize, Option<&'static str>) {
    let info = app.world().resource::<HistoryInfo>();
    (info.undo_depth, info.undo_label)
}

/// Every id the scene holds: bodies, joints and nodes.
fn all_ids(scene: &SceneRecord) -> Vec<StableId> {
    scene
        .bodies
        .iter()
        .map(|b| b.id)
        .chain(scene.joints.iter().map(|j| j.id))
        .chain(scene.nodes.iter().map(|n| n.id))
        .collect()
}

fn group_ids(scene: &SceneRecord) -> BTreeSet<u32> {
    scene
        .bodies
        .iter()
        .flat_map(|b| b.groups.iter().copied())
        .collect()
}

/// A fresh directory under the system temp dir for this test.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gradiance-r3-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Three balls: 0 and 1 hinged together and grouped, that group nested in
/// one with ball 2 (named "arm"), ball 2 pinned to the world.
fn mechanism(app: &mut App) {
    for x in [0.0, 1.0, 2.0] {
        op(app, "body.spawn_circle", json!({"x": x, "y": 1, "r": 0.3})).expect("spawns");
    }
    step(app, 2);
    op(app, "joint.hinge", json!({"a": 0, "b": 1, "x": 0.5, "y": 1})).expect("hinges");
    op(app, "joint.hinge", json!({"a": 2, "b": -1, "x": 2, "y": 2})).expect("pins");
    step(app, 2);
    op(app, "group.create", json!({"targets": [0, 1]})).expect("groups");
    step(app, 2);
    op(app, "group.create", json!({"targets": [0, 1, 2]})).expect("nests");
    step(app, 2);
    let outer = *group_ids(&capture(app)).last().expect("two groups");
    app.world_mut().write_message(NameGroupIntent {
        group: outer,
        name: "arm".to_owned(),
    });
    step(app, 2);
}

/// The new bodies' ids an insert or a paste returned.
fn returned_ids(value: Option<Value>) -> Vec<StableId> {
    value
        .and_then(|v| v.as_array().cloned())
        .expect("a list of ids")
        .iter()
        .map(|v| {
            let text = v.as_str().expect("an id string");
            StableId(text.parse().expect("a uuid"))
        })
        .collect()
}

/// Checks that `added` bodies of `after` copy `source`'s structure: the same
/// group stacks under a one-to-one renaming onto ids `before` does not use,
/// and joints between them as many as `source` has.
fn assert_copied_structure(
    source: &SceneRecord,
    before: &SceneRecord,
    after: &SceneRecord,
    added: &[StableId],
) {
    let added_bodies: Vec<_> = after
        .bodies
        .iter()
        .filter(|b| added.contains(&b.id))
        .collect();
    assert_eq!(added_bodies.len(), source.bodies.len());
    let used = group_ids(before);
    let mut rename: HashMap<u32, u32> = HashMap::new();
    // Bodies are inserted in the source's order, and `added` keeps it.
    for (src, id) in source.bodies.iter().zip(added) {
        let copy = added_bodies.iter().find(|b| b.id == *id).expect("added");
        assert_eq!(copy.groups.len(), src.groups.len(), "nesting depth kept");
        for (old, new) in src.groups.iter().zip(&copy.groups) {
            assert!(!used.contains(new), "group {new} joined an existing one");
            assert_eq!(*rename.entry(*old).or_insert(*new), *new, "one renaming");
        }
    }
    let inside = after
        .joints
        .iter()
        .filter(|j| j.def.referenced_bodies().all(|b| added.contains(&b)))
        .count();
    assert_eq!(inside, source.joints.len(), "the joints came along");
    // The outer group's name moved to its new id.
    for (old, name) in &source.environment.group_names.0 {
        if let Some(new) = rename.get(old) {
            assert_eq!(after.environment.group_names.get(*new), Some(name.as_str()));
        }
    }
}

#[test]
fn an_inserted_file_adds_fresh_objects_in_one_step() {
    let mut app = paused_app();
    mechanism(&mut app);
    let dir = temp_dir("insert");
    let path = dir.join("arm.ron");
    let path_text = path.to_string_lossy().into_owned();
    op(&mut app, "scene.save", json!({"path": path_text})).expect("saves");
    step(&mut app, 1);
    let file = from_ron(&std::fs::read_to_string(&path).expect("written")).expect("parses");
    assert_eq!(file.environment.group_names.0.len(), 1);

    // The file's own objects are still in the scene: every id it holds is
    // taken, so the insert must mint new ones.
    let before = capture(&mut app);
    let (depth, _) = history(&app);
    let added = returned_ids(
        op(&mut app, "scene.insert", json!({"path": path_text, "at": []})).expect("inserts"),
    );
    step(&mut app, 2);
    let after = capture(&mut app);
    assert_eq!(after.bodies.len(), 6);
    assert_eq!(after.joints.len(), 4);
    assert_eq!(history(&app), (depth + 1, Some("insert")), "one undo step");
    let ids = all_ids(&after);
    let distinct: BTreeSet<_> = ids.iter().map(|id| id.0).collect();
    assert_eq!(distinct.len(), ids.len(), "no id is used twice");
    for id in all_ids(&before) {
        assert!(!added.contains(&id), "an inserted body reused {id:?}");
    }
    assert_copied_structure(&file, &before, &after, &added);
    // `at: []` keeps the file's positions.
    for (src, id) in file.bodies.iter().zip(&added) {
        let copy = after.bodies.iter().find(|b| b.id == *id).expect("added");
        assert!(copy.pose.pos.distance(src.pose.pos) < 1e-5);
    }

    // Insert it again, centred elsewhere: three more, around (10, 0).
    let again = returned_ids(
        op(&mut app, "scene.insert", json!({"path": path_text, "at": [10, 0]}))
            .expect("inserts"),
    );
    step(&mut app, 2);
    let third = capture(&mut app);
    assert_eq!(third.bodies.len(), 9);
    let xs: Vec<Vec2> = third
        .bodies
        .iter()
        .filter(|b| again.contains(&b.id))
        .map(|b| b.pose.pos)
        .collect();
    let lo = xs.iter().fold(Vec2::MAX, |a, p| a.min(*p));
    let hi = xs.iter().fold(Vec2::MIN, |a, p| a.max(*p));
    assert!(((lo + hi) * 0.5).distance(Vec2::new(10.0, 0.0)) < 1e-4);
    assert_copied_structure(&file, &after, &third, &again);

    undo(&mut app);
    undo(&mut app);
    assert_eq!(capture(&mut app).bodies.len(), 3, "each insert undoes whole");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_bad_insert_is_an_error_and_changes_nothing() {
    let mut app = paused_app();
    let (depth, _) = history(&app);
    assert!(op(&mut app, "scene.insert", json!({"path": "no/such.ron", "at": []})).is_err());
    assert!(op(&mut app, "edit.paste", json!({"text": "not ron", "at": []})).is_err());
    assert!(
        op(
            &mut app,
            "scene.insert",
            json!({"path": "tests/fixtures/scenes/pendulum.ron", "at": [1]})
        )
        .is_err()
    );
    step(&mut app, 2);
    assert_eq!(history(&app).0, depth);
    assert!(capture(&mut app).bodies.is_empty());
}

#[test]
fn copy_then_paste_adds_the_set_at_the_place() {
    let mut app = paused_app();
    mechanism(&mut app);
    // Balls 0 and 1: their hinge comes along, ball 2's pin does not.
    let text = op(&mut app, "edit.copy", json!({"targets": [0, 1]}))
        .expect("copies")
        .and_then(|v| v.as_str().map(str::to_owned))
        .expect("the text");
    step(&mut app, 1);
    assert_eq!(
        app.world().resource::<LastCopy>().0.as_deref(),
        Some(text.as_str()),
        "the copy reached the clipboard bridge"
    );
    let copied = from_ron(&text).expect("the copy is scene text");
    assert_eq!(copied.bodies.len(), 2);
    assert_eq!(copied.joints.len(), 1);
    assert_eq!(copied.environment.group_names.0.len(), 1, "the outer name");

    let before = capture(&mut app);
    let (depth, _) = history(&app);
    let added = returned_ids(
        op(&mut app, "edit.paste", json!({"text": text, "at": [-4, 3]})).expect("pastes"),
    );
    step(&mut app, 2);
    let after = capture(&mut app);
    assert_eq!(history(&app), (depth + 1, Some("insert")), "one undo step");
    assert_eq!(after.bodies.len(), 5);
    assert_copied_structure(&copied, &before, &after, &added);
    let centre = after
        .bodies
        .iter()
        .filter(|b| added.contains(&b.id))
        .fold(Vec2::ZERO, |a, b| a + b.pose.pos * 0.5);
    assert!(centre.distance(Vec2::new(-4.0, 3.0)) < 1e-4, "{centre}");

    // A second paste of the same text: new ids again.
    let twice = returned_ids(
        op(&mut app, "edit.paste", json!({"text": text, "at": []})).expect("pastes"),
    );
    step(&mut app, 2);
    let ids = all_ids(&capture(&mut app));
    let distinct: BTreeSet<_> = ids.iter().map(|id| id.0).collect();
    assert_eq!(distinct.len(), ids.len(), "no id is used twice");
    assert!(twice.iter().all(|id| !added.contains(id)));
}

/// Presses keys for one frame, then releases them.
fn press(app: &mut App, keys: &[KeyCode]) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    for k in keys {
        input.press(*k);
    }
    app.update();
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    for k in keys {
        input.release(*k);
    }
    app.update();
}

fn select_all(app: &mut App) {
    press(app, &[KeyCode::ControlLeft, KeyCode::KeyA]);
}

#[test]
fn ctrl_c_and_ctrl_v_copy_the_selection_and_paste_it_at_the_cursor() {
    let mut app = paused_app();
    mechanism(&mut app);
    select_all(&mut app);
    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyC]);
    let text = app.world().resource::<LastCopy>().0.clone().expect("copied");
    assert_eq!(from_ron(&text).expect("scene text").bodies.len(), 3);
    assert_eq!(
        *app.world().resource::<State<ToolState>>().get(),
        ToolState::Select,
        "Ctrl+C is not the circle tool's C"
    );

    let (depth, _) = history(&app);
    app.insert_resource(CursorWorldPos(Some(Vec2::new(5.0, 5.0))));
    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyV]);
    step(&mut app, 1);
    let after = capture(&mut app);
    assert_eq!(after.bodies.len(), 6, "pasted");
    assert_eq!(after.joints.len(), 4);
    assert_eq!(history(&app), (depth + 1, Some("insert")), "one undo step");
    let pasted: Vec<Vec2> = after
        .bodies
        .iter()
        .filter(|b| b.pose.pos.y > 4.0)
        .map(|b| b.pose.pos)
        .collect();
    assert_eq!(pasted.len(), 3, "around the cursor");

    // A number field under the pointer keeps Ctrl+C and Ctrl+V.
    app.world_mut().resource_mut::<LastCopy>().0 = None;
    app.insert_resource(KeyboardCaptured(true));
    app.insert_resource(FieldKeys(true));
    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyC]);
    assert_eq!(app.world().resource::<LastCopy>().0, None, "the field's copy");
    app.world_mut().resource_mut::<LastCopy>().0 = Some(text);
    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyV]);
    step(&mut app, 1);
    assert_eq!(capture(&mut app).bodies.len(), 6, "the field's paste");
}
