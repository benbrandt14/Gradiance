//! Body booleans (M21, briefs 4-1 and 4-2): subtract / intersect / xor
//! and the filleted merge (smooth union) between bodies via
//! `BooleanIntent`. The host's tree is checked by containment
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
                anchor_a: gradiance::units::Displacement(anchor),
                anchor_b: gradiance::units::Displacement(world),
                rest_rot_a: gradiance::units::Angle(0.0),
                rest_rot_b: gradiance::units::Angle(0.0),
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
        joint(&mut app, kept).map(|d| (d.body_a, d.anchor_a.value())),
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
                anchor_a: gradiance::units::Displacement(Vec2::new(-0.5, 0.5)),
                anchor_b: gradiance::units::Displacement(Vec2::new(0.0, -2.5)),
                rest_rot_a: gradiance::units::Angle(0.0),
                rest_rot_b: gradiance::units::Angle(0.0),
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
                anchor_a: gradiance::units::Displacement(Vec2::new(0.5, 0.0)),
                anchor_b: gradiance::units::Displacement(Vec2::new(-0.5, 0.0)),
                rest_rot_a: gradiance::units::Angle(0.0),
                rest_rot_b: gradiance::units::Angle(0.0),
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
        (rewired.anchor_a.value() - Vec2::new(0.5, 0.5)).length() < 1e-4,
        "at the same world point, in host space (got {})",
        rewired.anchor_a.value()
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
        l.anchor_a.value().length() < 0.02,
        "anchored at that piece's centre"
    );
    let r = joint(&mut app, right).expect("the right pin follows its piece");
    assert_eq!(r.body_a, piece_at(1.5));
    assert!(r.anchor_a.value().length() < 0.02);
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
        (CsgOp::SmoothUnion { radius: 0.2 }, "fillet-merge"),
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

// --- Filleted merge (brief 4-2): `CsgOp::SmoothUnion` through the same intent ---

/// A filleted merge of an L (a bar and an overlapping post) is one body
/// whose field *is* the smooth union of the two boxes, placed in the
/// host's frame; it fills the concave corner a plain union leaves sharp;
/// one undo restores both bodies.
#[test]
fn a_fillet_merge_of_two_overlapping_boxes_is_their_smooth_union() {
    let mut app = paused_app();
    let host_pos = Vec2::new(1.0, 0.5);
    let bar = spawn_box(&mut app, host_pos, 2.0, 0.5);
    let post_offset = Vec2::new(-0.75, 0.9);
    let post = spawn_box(&mut app, host_pos + post_offset, 0.5, 1.5);
    let before = SceneRecord::capture(app.world_mut());
    let steps = depth(&app);
    let radius = 0.3;

    boolean(&mut app, CsgOp::SmoothUnion { radius }, vec![bar, post]);

    assert_eq!(depth(&app), steps + 1, "one undo step");
    assert_eq!(
        app.world().resource::<CommandStack>().peek_undo(),
        Some("fillet-merge")
    );
    assert_eq!(body_count(&mut app), 1, "the post is consumed");
    assert!(entity_of(&app, post).is_none());
    let shape = shape_of(&app, bar);
    let expected = ShapeDef::Csg {
        op: CsgOp::SmoothUnion { radius },
        lhs: Box::new(ShapeDef::Box {
            width: 2.0,
            height: 0.5,
        }),
        rhs: Box::new(ShapeDef::Placed {
            pos: post_offset,
            rot: 0.0,
            shape: Box::new(ShapeDef::Box {
                width: 0.5,
                height: 1.5,
            }),
        }),
    };
    for ix in -12..=12 {
        for iy in -6..=12 {
            let p = Vec2::new(ix as f32, iy as f32) * 0.1;
            let (got, want) = (sdf::eval(&shape, p), sdf::eval(&expected, p));
            assert!((got - want).abs() < 1e-5, "field at {p}: {got} vs {want}");
        }
    }
    // The concave corner (host-local (-0.5, 0.25)) is filled just off
    // the corner, where a plain union is empty.
    let in_fillet = Vec2::new(-0.47, 0.28);
    assert!(sdf::eval(&shape, in_fillet) < 0.0, "the fillet is solid");
    if let ShapeDef::Csg { lhs, rhs, .. } = expected.clone() {
        let plain = ShapeDef::Csg {
            op: CsgOp::Union,
            lhs,
            rhs,
        };
        assert!(sdf::eval(&plain, in_fillet) > 0.0, "a plain union is sharp");
    }
    // The blended body still builds a collider.
    app.update();
    let entity = entity_of(&app, bar).unwrap();
    assert!(
        app.world()
            .get::<avian2d::prelude::Collider>(entity)
            .is_some()
    );

    undo(&mut app);
    assert_eq!(
        before,
        SceneRecord::capture(app.world_mut()),
        "one undo restores both bodies"
    );
    redo(&mut app);
    assert_eq!(body_count(&mut app), 1, "redo replays the fillet merge");
}

/// A smooth union's blend closes a gap between parallel faces narrower
/// than `radius / 2`, so a filleted merge joins bodies a plain merge
/// refuses as disconnected; a gap of more than half the radius stays
/// open and is refused like any disconnected union.
#[test]
fn a_fillet_merge_joins_bodies_a_gap_apart() {
    // Two unit boxes, `gap` apart along x; fillet radius 1.
    let pair = |gap: f32| {
        let mut app = paused_app();
        let a = spawn_box(&mut app, Vec2::ZERO, 1.0, 1.0);
        let b = spawn_box(&mut app, Vec2::new(1.0 + gap, 0.0), 1.0, 1.0);
        (app, a, b)
    };

    let (mut app, a, b) = pair(0.3);
    let steps = depth(&app);
    app.world_mut().write_message(MergeIntent {
        targets: vec![a, b],
    });
    app.update();
    assert_eq!(depth(&app), steps, "a plain merge refuses the gap");
    boolean(&mut app, CsgOp::SmoothUnion { radius: 1.0 }, vec![a, b]);
    assert_eq!(depth(&app), steps + 1, "the fillet bridges it");
    assert_eq!(body_count(&mut app), 1);
    let shape = shape_of(&app, a);
    assert!(
        sdf::eval(&shape, Vec2::new(0.65, 0.0)) < 0.0,
        "the gap is filled"
    );
    assert_eq!(polygonize_components(&shape).len(), 1);

    let (mut app, a, b) = pair(0.7);
    let steps = depth(&app);
    boolean(&mut app, CsgOp::SmoothUnion { radius: 1.0 }, vec![a, b]);
    assert_eq!(depth(&app), steps, "0.7 m is over half the radius: refused");
    assert_eq!(body_count(&mut app), 2);
}

/// A fillet radius that is zero, negative, NaN or infinite is refused by
/// the command itself (whatever surface emitted it): no history, no body
/// touched.
#[test]
fn a_bad_fillet_radius_commits_nothing() {
    let mut app = paused_app();
    let a = spawn_box(&mut app, Vec2::ZERO, 2.0, 0.5);
    let b = spawn_box(&mut app, Vec2::new(0.5, 0.5), 0.5, 1.0);
    let before = SceneRecord::capture(app.world_mut());
    let steps = depth(&app);
    for radius in [0.0, -0.25, f32::NAN, f32::INFINITY] {
        boolean(&mut app, CsgOp::SmoothUnion { radius }, vec![a, b]);
        assert_eq!(depth(&app), steps, "radius {radius}: nothing recorded");
        assert_eq!(
            before,
            SceneRecord::capture(app.world_mut()),
            "radius {radius}: nothing changed"
        );
    }
}
