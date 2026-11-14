//! Body booleans (M21, brief 4-1): subtract / intersect / xor between
//! bodies via `BooleanIntent`. The host's tree is checked by containment
//! at sample points; operands are consumed; joints are rewired or
//! deleted; one undo restores the scene record-for-record.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{body_count, box_record, entity_of, paused_app, redo, undo};
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::geometry::polygonize::polygonize_components;
use gradiance::geometry::sdf;
use gradiance::prelude::*;

fn spawn(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn spawn_box(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    spawn(app, box_record(pos, w, h))
}

fn spawn_circle(app: &mut App, pos: Vec2, radius: f32) -> StableId {
    let mut record = box_record(pos, 1.0, 1.0);
    record.shape = ShapeDef::Circle { radius };
    spawn(app, record)
}

/// A world pin on `body` at its local `anchor` (world point `world`).
fn pin(app: &mut App, body: StableId, anchor: Vec2, world: Vec2) -> StableId {
    let id = StableId::new();
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id,
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: body,
                body_b: None,
                anchor_a: anchor,
                anchor_b: world,
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();
    id
}

fn boolean(app: &mut App, op: CsgOp, targets: Vec<StableId>) {
    app.world_mut().write_message(BooleanIntent { op, targets });
    app.update();
}

fn shape_of(app: &App, id: StableId) -> ShapeDef {
    let entity = entity_of(app, id).expect("body is live");
    app.world().get::<ShapeDef>(entity).unwrap().clone()
}

fn joint_defs(app: &mut App) -> Vec<(StableId, JointDef)> {
    app.world_mut()
        .query::<(&StableId, &JointDef)>()
        .iter(app.world())
        .map(|(id, def)| (*id, def.clone()))
        .collect()
}

fn joint(app: &mut App, id: StableId) -> Option<JointDef> {
    joint_defs(app)
        .into_iter()
        .find(|(j, _)| *j == id)
        .map(|(_, def)| def)
}

fn depth(app: &App) -> usize {
    app.world().resource::<CommandStack>().undo_len()
}

/// Asserts `shape` is solid at each of `inside` and empty at each of
/// `outside` (host-local points).
fn assert_containment(shape: &ShapeDef, inside: &[Vec2], outside: &[Vec2]) {
    for p in inside {
        assert!(sdf::eval(shape, *p) < 0.0, "expected solid at {p}");
    }
    for p in outside {
        assert!(sdf::eval(shape, *p) > 0.0, "expected empty at {p}");
    }
}

#[test]
fn subtracting_a_circle_punches_an_analytic_hole() {
    let mut app = paused_app();
    let host = spawn_box(&mut app, Vec2::ZERO, 2.0, 2.0);
    let tool = spawn_circle(&mut app, Vec2::new(0.2, 0.0), 0.5);
    let kept = pin(&mut app, host, Vec2::new(0.85, 0.0), Vec2::new(0.85, 0.0));
    let in_hole = pin(&mut app, host, Vec2::new(0.3, 0.0), Vec2::new(0.3, 0.0));
    let on_tool = pin(&mut app, tool, Vec2::ZERO, Vec2::new(0.2, 0.0));
    let before = SceneRecord::capture(app.world_mut());
    let steps = depth(&app);

    boolean(&mut app, CsgOp::Subtract, vec![host, tool]);

    assert_eq!(depth(&app), steps + 1, "one undo step");
    assert_eq!(body_count(&mut app), 1, "the tool body is consumed");
    assert!(entity_of(&app, tool).is_none());
    let shape = shape_of(&app, host);
    assert!(
        matches!(
            shape,
            ShapeDef::Csg {
                op: CsgOp::Subtract,
                ..
            }
        ),
        "an analytic subtract tree, got {shape:?}"
    );
    assert_containment(
        &shape,
        &[
            Vec2::new(0.85, 0.0),
            Vec2::new(-0.5, 0.0),
            Vec2::new(0.2, 0.7),
        ],
        &[
            Vec2::new(0.2, 0.0),
            Vec2::new(0.6, 0.0),
            Vec2::new(1.2, 0.0),
        ],
    );
    assert_eq!(
        joint(&mut app, kept).map(|d| (d.body_a, d.anchor_a)),
        Some((host, Vec2::new(0.85, 0.0))),
        "a pin on surviving material is untouched"
    );
    assert!(
        joint(&mut app, in_hole).is_none(),
        "a pin whose material was removed is deleted"
    );
    assert!(
        joint(&mut app, on_tool).is_none(),
        "a pin on the consumed tool's (removed) material is deleted"
    );

    undo(&mut app);
    assert_eq!(
        before,
        SceneRecord::capture(app.world_mut()),
        "one undo restores everything"
    );
    redo(&mut app);
    assert_eq!(body_count(&mut app), 1, "redo replays the subtract");
}

#[test]
fn intersect_keeps_the_overlap_and_rewires_the_operands_joints() {
    let mut app = paused_app();
    let host = spawn_box(&mut app, Vec2::ZERO, 2.0, 2.0);
    let other = spawn_box(&mut app, Vec2::new(1.0, 0.0), 2.0, 2.0);
    let third = spawn_box(&mut app, Vec2::new(0.5, 3.0), 0.5, 0.5);
    // A hinge from the consumed operand (at world (0.5, 0.5), inside the
    // overlap) to an outside body.
    let hinge = StableId::new();
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: hinge,
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: other,
                body_b: Some(third),
                anchor_a: Vec2::new(-0.5, 0.5),
                anchor_b: Vec2::new(0.0, -2.5),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();
    let outside_overlap = pin(&mut app, host, Vec2::new(-0.5, 0.0), Vec2::new(-0.5, 0.0));
    let internal = StableId::new();
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: internal,
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: host,
                body_b: Some(other),
                anchor_a: Vec2::new(0.5, 0.0),
                anchor_b: Vec2::new(-0.5, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();
    let before = SceneRecord::capture(app.world_mut());

    boolean(&mut app, CsgOp::Intersect, vec![host, other]);

    assert_eq!(body_count(&mut app), 2, "the operand is consumed");
    assert!(entity_of(&app, other).is_none());
    let shape = shape_of(&app, host);
    assert!(matches!(
        shape,
        ShapeDef::Csg {
            op: CsgOp::Intersect,
            ..
        }
    ));
    assert_containment(
        &shape,
        &[Vec2::new(0.5, 0.0), Vec2::new(0.1, 0.9)],
        &[Vec2::new(-0.5, 0.0), Vec2::new(1.5, 0.0)],
    );
    let rewired = joint(&mut app, hinge).expect("the operand's hinge survives");
    assert_eq!(rewired.body_a, host, "rewired onto the host");
    assert!(
        (rewired.anchor_a - Vec2::new(0.5, 0.5)).length() < 1e-4,
        "at the same world point, in host space (got {})",
        rewired.anchor_a
    );
    assert_eq!(rewired.body_b, Some(third), "the other end is untouched");
    assert!(
        joint(&mut app, outside_overlap).is_none(),
        "a pin outside the overlap lost its material"
    );
    assert!(
        joint(&mut app, internal).is_none(),
        "a joint between the operands is internal and deleted"
    );

    undo(&mut app);
    assert_eq!(before, SceneRecord::capture(app.world_mut()));
}

#[test]
fn xor_around_a_contained_circle_is_one_analytic_body() {
    let mut app = paused_app();
    let host = spawn_box(&mut app, Vec2::ZERO, 2.0, 2.0);
    let inner = spawn_circle(&mut app, Vec2::ZERO, 0.4);
    let before = SceneRecord::capture(app.world_mut());

    boolean(&mut app, CsgOp::Xor, vec![host, inner]);

    assert_eq!(body_count(&mut app), 1);
    let shape = shape_of(&app, host);
    assert!(matches!(shape, ShapeDef::Csg { op: CsgOp::Xor, .. }));
    assert_containment(
        &shape,
        &[Vec2::new(0.7, 0.0), Vec2::new(-0.7, -0.7)],
        &[Vec2::ZERO, Vec2::new(0.3, 0.0), Vec2::new(1.1, 0.0)],
    );
    undo(&mut app);
    assert_eq!(before, SceneRecord::capture(app.world_mut()));
}

/// The disconnected-result decision: subtract / intersect / xor may split
/// the host, which is then replaced by polygon pieces (as a severing cut
/// does); joints follow their anchors onto the piece that contains them.
#[test]
fn xor_of_overlapping_boxes_splits_into_pieces() {
    let mut app = paused_app();
    let host = spawn_box(&mut app, Vec2::ZERO, 2.0, 2.0);
    let other = spawn_box(&mut app, Vec2::new(1.0, 0.0), 2.0, 2.0);
    let left = pin(&mut app, host, Vec2::new(-0.5, 0.0), Vec2::new(-0.5, 0.0));
    let right = pin(&mut app, other, Vec2::new(0.5, 0.0), Vec2::new(1.5, 0.0));
    let shared = pin(&mut app, host, Vec2::new(0.5, 0.0), Vec2::new(0.5, 0.0));
    let before = SceneRecord::capture(app.world_mut());
    let steps = depth(&app);

    boolean(&mut app, CsgOp::Xor, vec![host, other]);

    assert_eq!(depth(&app), steps + 1, "one undo step");
    assert!(
        entity_of(&app, host).is_none(),
        "the split host is replaced"
    );
    assert!(entity_of(&app, other).is_none(), "the operand is consumed");
    let pieces: Vec<(StableId, Transform, ShapeDef)> = app
        .world_mut()
        .query_filtered::<(&StableId, &Transform, &ShapeDef), With<Body>>()
        .iter(app.world())
        .map(|(id, t, s)| (*id, *t, s.clone()))
        .collect();
    assert_eq!(pieces.len(), 2, "the two private halves");
    for (_, transform, shape) in &pieces {
        assert!(matches!(shape, ShapeDef::Polygon { .. }));
        let area: f32 = polygonize_components(shape)
            .iter()
            .map(gradiance::geometry::contours::Contours::area)
            .sum();
        assert!((area - 2.0).abs() < 0.05, "each half is 1 x 2 (got {area})");
        let x = transform.translation.x;
        assert!(
            (x + 0.5).abs() < 0.02 || (x - 1.5).abs() < 0.02,
            "pieces sit at the halves' centroids (got {x})"
        );
    }
    let piece_at = |x: f32| {
        pieces
            .iter()
            .find(|(_, t, _)| (t.translation.x - x).abs() < 0.02)
            .map(|(id, ..)| *id)
            .unwrap()
    };
    let l = joint(&mut app, left).expect("the left pin follows its piece");
    assert_eq!(l.body_a, piece_at(-0.5));
    assert!(
        l.anchor_a.length() < 0.02,
        "anchored at that piece's centre"
    );
    let r = joint(&mut app, right).expect("the right pin follows its piece");
    assert_eq!(r.body_a, piece_at(1.5));
    assert!(r.anchor_a.length() < 0.02);
    assert!(
        joint(&mut app, shared).is_none(),
        "a pin in the removed overlap is deleted"
    );

    undo(&mut app);
    assert_eq!(before, SceneRecord::capture(app.world_mut()));
}

#[test]
fn subtracting_a_bar_across_a_box_splits_it() {
    let mut app = paused_app();
    let host = spawn_box(&mut app, Vec2::ZERO, 2.0, 1.0);
    let bar = spawn_box(&mut app, Vec2::ZERO, 0.2, 3.0);

    boolean(&mut app, CsgOp::Subtract, vec![host, bar]);

    assert_eq!(body_count(&mut app), 2, "two pieces, the bar consumed");
    assert!(entity_of(&app, host).is_none());
    assert!(entity_of(&app, bar).is_none());
}

#[test]
fn booleans_with_nothing_to_do_are_refused() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    let far = spawn_box(&mut app, Vec2::new(5.0, 0.0), 1.0, 1.0);
    let cover = spawn_box(&mut app, Vec2::new(0.0, 0.0), 3.0, 3.0);
    let before = SceneRecord::capture(app.world_mut());
    let steps = depth(&app);

    for (op, targets, why) in [
        (CsgOp::Intersect, vec![a, far], "an empty intersection"),
        (CsgOp::Subtract, vec![a, far], "a subtract that misses"),
        (CsgOp::Xor, vec![a, far], "an xor of disjoint bodies"),
        (
            CsgOp::Subtract,
            vec![a, cover],
            "a subtract that swallows the host",
        ),
        (CsgOp::Subtract, vec![a, a], "a body against itself"),
        (CsgOp::Subtract, vec![a], "a single body"),
    ] {
        boolean(&mut app, op, targets);
        assert_eq!(depth(&app), steps, "refused: {why}");
        assert_eq!(before, SceneRecord::capture(app.world_mut()), "{why}");
    }
}

#[test]
fn the_ground_is_not_an_operand() {
    let mut app = paused_app();
    let mut ground = box_record(Vec2::ZERO, 1.0, 1.0);
    ground.shape = ShapeDef::HalfPlane;
    let ground = spawn(&mut app, ground);
    let a = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
    let steps = depth(&app);
    boolean(&mut app, CsgOp::Subtract, vec![ground, a]);
    boolean(&mut app, CsgOp::Subtract, vec![a, ground]);
    assert_eq!(depth(&app), steps);
    assert_eq!(body_count(&mut app), 2);
}

/// Every boolean is one undo step labelled by its op.
#[test]
fn each_op_is_labelled_in_history() {
    for (op, label) in [
        (CsgOp::Subtract, "subtract"),
        (CsgOp::Intersect, "intersect"),
        (CsgOp::Xor, "xor"),
        (CsgOp::Union, "merge"),
    ] {
        let mut app = paused_app();
        let a = spawn_box(&mut app, Vec2::ZERO, 2.0, 2.0);
        let b = spawn_circle(&mut app, Vec2::new(0.1, 0.0), 0.5);
        boolean(&mut app, op, vec![a, b]);
        assert_eq!(
            app.world().resource::<CommandStack>().peek_undo(),
            Some(label)
        );
    }
}

#[test]
fn intersecting_a_box_with_a_contained_circle_leaves_the_circle() {
    let mut app = paused_app();
    let host = spawn_box(&mut app, Vec2::ZERO, 2.0, 2.0);
    let disc = spawn_circle(&mut app, Vec2::ZERO, 0.4);
    boolean(&mut app, CsgOp::Intersect, vec![host, disc]);
    assert_eq!(body_count(&mut app), 1);
    let shape = shape_of(&app, host);
    assert_containment(
        &shape,
        &[Vec2::ZERO, Vec2::new(0.35, 0.0)],
        &[Vec2::new(0.45, 0.0), Vec2::new(0.9, 0.9)],
    );
    // A near-circular contour must still build a collider (a fine ring
    // used to decompose to no convex part and panic in parry).
    app.update();
    let entity = entity_of(&app, host).unwrap();
    assert!(
        app.world()
            .get::<avian2d::prelude::Collider>(entity)
            .is_some()
    );
}
