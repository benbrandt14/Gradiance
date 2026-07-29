//! Intent-replay golden scenarios: replay an intent stream through a paused
//! headless app and compare the final authored world (normalized scene RON)
//! against `tests/it/golden/`. Snapshots re-key apply-time-minted ids
//! (cut pieces, clones) from a content sort so they stay deterministic.
//! Regenerate intended changes with `UPDATE_GOLDEN=1 cargo test --test it
//! replay::` and review the diff.

// Test-binary helpers: panics are the failure mechanism (clippy's
// allow-*-in-tests config covers #[test] fns, not their helpers).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crate::harness::{box_record, paused_app};
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::scene::{JointRecord, SceneRecord};
use std::path::PathBuf;
use uuid::Uuid;

/// A deterministic id for scenario-authored records.
fn id(n: u128) -> StableId {
    StableId(Uuid::from_u128(n))
}

/// Sends one intent message and steps a frame (dispatch + syncs run).
fn send<M: Message>(app: &mut App, intent: M) {
    app.world_mut().write_message(intent);
    app.update();
}

/// Captures the authored world as normalized, deterministic RON.
fn snapshot(app: &mut App) -> String {
    let mut scene = SceneRecord::capture(app.world_mut());
    "golden".clone_into(&mut scene.app_version);
    normalize_ids(&mut scene);
    gradiance::scene::to_ron(&scene).expect("scene serializes")
}

/// Re-keys every `StableId` from a deterministic content ordering, so ids
/// minted at apply time (cut pieces, duplicate clones) don't break the
/// golden compare. Scenarios keep body poses distinct by construction.
fn normalize_ids(scene: &mut SceneRecord) {
    let mut order: Vec<usize> = (0..scene.bodies.len()).collect();
    let key = |b: &gradiance::scene::BodyRecord| {
        (
            (b.pose.pos.x * 1000.0).round() as i64,
            (b.pose.pos.y * 1000.0).round() as i64,
            format!("{:?}", b.shape),
        )
    };
    order.sort_by_key(|&i| key(&scene.bodies[i]));

    let mut remap: Vec<(StableId, StableId)> = Vec::new();
    for (new_index, &old_index) in order.iter().enumerate() {
        let fresh = id(1 + new_index as u128);
        remap.push((scene.bodies[old_index].id, fresh));
        scene.bodies[old_index].id = fresh;
    }
    let mapped = |old: StableId| -> StableId {
        remap
            .iter()
            .find(|(from, _)| *from == old)
            .map_or(old, |(_, to)| *to)
    };

    for joint in &mut scene.joints {
        joint.def.body_a = mapped(joint.def.body_a);
        joint.def.body_b = joint.def.body_b.map(mapped);
    }
    let mut joint_order: Vec<usize> = (0..scene.joints.len()).collect();
    joint_order.sort_by_key(|&i| {
        let d = &scene.joints[i].def;
        (
            d.body_a.0,
            (d.anchor_a.x * 1000.0).round() as i64,
            (d.anchor_a.y * 1000.0).round() as i64,
        )
    });
    for (new_index, &old_index) in joint_order.iter().enumerate() {
        scene.joints[old_index].id = id(1000 + new_index as u128);
    }

    // Capture sorts by id; re-sort so the normalized ids are also the
    // file order.
    scene.bodies.sort_by_key(|r| r.id.0);
    scene.joints.sort_by_key(|r| r.id.0);
}

/// Asserts `actual` matches the named golden file (or rewrites it under
/// `UPDATE_GOLDEN=1`).
fn assert_golden(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/it/golden")
        .join(format!("{name}.ron"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("mkdir golden");
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {} ({e}); run UPDATE_GOLDEN=1 cargo test --test it replay::",
            path.display()
        )
    });
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "replay diverged from golden {name}; if intended, regenerate with \
         UPDATE_GOLDEN=1 and review the diff"
    );
}

/// A limited, motorized hinge between two bodies.
fn hinge(a: StableId, b: StableId, anchor_a: Vec2, anchor_b: Vec2) -> JointDef {
    JointDef {
        kind: JointKind::Hinge {
            limits: Some([-1.0, 1.0]),
            motor: Some(AngularMotorDef::default()),
        },
        common: JointCommon::default(),
        body_a: a,
        body_b: Some(b),
        anchor_a,
        anchor_b,
        rest_rot_a: 0.0,
        rest_rot_b: 0.0,
    }
}

fn spawn_joint(app: &mut App, id: StableId, def: JointDef) {
    send(
        app,
        SpawnJointIntent {
            record: JointRecord { id, def },
        },
    );
}

/// Draft-tool family: spawn box/circle/ground, then cut the box in two.
#[test]
fn replay_draft_tools() {
    let mut app = paused_app();
    send(
        &mut app,
        SpawnBodyIntent {
            record: box_record(Vec2::new(0.0, 100.0), 80.0, 40.0),
        },
    );
    let mut circle = box_record(Vec2::new(200.0, 100.0), 0.0, 0.0);
    circle.id = id(2);
    circle.shape = ShapeDef::Circle { radius: 25.0 };
    send(&mut app, SpawnBodyIntent { record: circle });
    let mut ground = box_record(Vec2::new(0.0, -50.0), 0.0, 0.0);
    ground.id = id(3);
    ground.shape = ShapeDef::HalfPlane;
    ground.physics = BodyPhysics::fixed();
    send(&mut app, SpawnBodyIntent { record: ground });

    // Vertical stroke through the box severs it into two pieces.
    send(
        &mut app,
        CutIntent {
            a: Vec2::new(0.0, 160.0),
            b: Vec2::new(0.0, 40.0),
            width: 4.0,
        },
    );

    assert_golden("replay_draft_tools", &snapshot(&mut app));
}

/// Manipulation family: move/rotate commit, scale, duplicate — then verify
/// the whole stream unwinds and replays through undo/redo.
#[test]
fn replay_manip_with_undo_redo() {
    let mut app = paused_app();
    let a = box_record(Vec2::ZERO, 40.0, 20.0);
    let (a_id, a_pose) = (a.id, a.pose);
    send(&mut app, SpawnBodyIntent { record: a });
    let mut b = box_record(Vec2::new(150.0, 0.0), 30.0, 30.0);
    b.id = id(2);
    let b_id = b.id;
    send(&mut app, SpawnBodyIntent { record: b });
    let baseline = snapshot(&mut app);

    send(
        &mut app,
        CommitTransformIntent {
            changes: vec![TransformChange {
                id: a_id,
                old: a_pose,
                new: PosRot {
                    pos: Vec2::new(-60.0, 25.0),
                    rot: 0.5,
                },
            }],
        },
    );
    send(
        &mut app,
        ScaleIntent {
            targets: vec![b_id],
            pivot: Vec2::new(150.0, 0.0),
            frame_rot: 0.0,
            factors: Vec2::new(2.0, 0.5),
        },
    );
    send(
        &mut app,
        DuplicateIntent {
            sources: vec![a_id, b_id],
            offset: Vec2::new(0.0, 300.0),
        },
    );
    let after = snapshot(&mut app);
    assert_golden("replay_manip", &after);

    // Undo the three edits: back to the two freshly spawned boxes.
    for _ in 0..3 {
        send(&mut app, UndoIntent);
    }
    assert_eq!(snapshot(&mut app), baseline, "undo unwinds to the baseline");
    // Redo them all: identical to the golden (ids must survive the cycle).
    for _ in 0..3 {
        send(&mut app, RedoIntent);
    }
    assert_eq!(snapshot(&mut app), after, "redo replays to the same state");
}

/// Joint family: hinge with motor + world-pin spring, then a property edit
/// and a joint delete.
#[test]
fn replay_joints() {
    let mut app = paused_app();
    let a = box_record(Vec2::ZERO, 40.0, 20.0);
    let a_id = a.id;
    send(&mut app, SpawnBodyIntent { record: a });
    let mut b = box_record(Vec2::new(100.0, 0.0), 40.0, 20.0);
    b.id = id(2);
    let b_id = b.id;
    send(&mut app, SpawnBodyIntent { record: b });

    spawn_joint(
        &mut app,
        id(10),
        hinge(a_id, b_id, Vec2::new(20.0, 0.0), Vec2::new(-20.0, 0.0)),
    );
    // World-pin spring on body B.
    spawn_joint(
        &mut app,
        id(11),
        JointDef {
            kind: JointKind::Spring {
                rest_length: 50.0,
                stiffness: gradiance::domain::joint::DEFAULT_SPRING_STIFFNESS,
                damping: gradiance::domain::joint::DEFAULT_SPRING_DAMPING,
                range: None,
            },
            common: JointCommon::default(),
            body_a: b_id,
            body_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::new(100.0, 80.0),
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );
    // Property edit through the same seam the inspector uses.
    send(
        &mut app,
        PropertyEditIntent {
            changes: vec![PropertyChange {
                id: a_id,
                old: PropertyValue::Density(Density(1.0)),
                new: PropertyValue::Density(Density(3.5)),
            }],
        },
    );
    // Delete the spring again (exercises DeleteJointCommand).
    send(&mut app, DeleteJointIntent { id: id(11) });

    assert_golden("replay_joints", &snapshot(&mut app));
}

/// Group/merge family: group two boxes, merge two others, ungroup.
#[test]
fn replay_group_merge() {
    let mut app = paused_app();
    let mut ids = Vec::new();
    for (n, pos) in [
        Vec2::new(0.0, 0.0),
        Vec2::new(30.0, 0.0), // overlaps the first → mergeable
        Vec2::new(200.0, 0.0),
        Vec2::new(300.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut record = box_record(pos, 40.0, 20.0);
        record.id = id(1 + n as u128);
        ids.push(record.id);
        send(&mut app, SpawnBodyIntent { record });
    }

    send(
        &mut app,
        GroupIntent {
            targets: vec![ids[2], ids[3]],
        },
    );
    send(
        &mut app,
        MergeIntent {
            targets: vec![ids[0], ids[1]],
        },
    );
    send(
        &mut app,
        UngroupIntent {
            targets: vec![ids[2]],
        },
    );

    assert_golden("replay_group_merge", &snapshot(&mut app));
}
