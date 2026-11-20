//! Mirror H / V and the joint load colour (brief N-9).
//!
//! Mirror is one operation over the existing negative scale: one undo step
//! that takes the selection's joints, anchors and nodes along, and mirroring
//! twice gives the scene back. The load colour is derived from the joint's
//! reaction force and is never saved.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::joint::{AngularMotorDef, HOT_LOAD};
use gradiance::domain::node::NodeKind;
use gradiance::domain::settings::SimSettings;
use gradiance::op::{InvokeOp, LastOp, Value, json};
use gradiance::prelude::*;
use gradiance::render::joint_viz::{JointLoad, load_colour};
use gradiance::units::{Angle, AngularVelocity, Displacement, Force};

fn invoke(app: &mut App, id: &'static str, params: Value) {
    app.world_mut().write_message(InvokeOp::with(id, params));
    step(app, 2);
}

fn spawn(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn spawn_joint(app: &mut App, def: JointDef) -> StableId {
    let record = JointRecord {
        id: StableId::new(),
        def,
    };
    let id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    id
}

fn record(app: &mut App) -> SceneRecord {
    let mut record = SceneRecord::capture(app.world_mut());
    // Settings are not what a mirror touches; keep the comparison on the
    // authored entities.
    record.environment = EnvironmentRecord::default();
    record
}

fn joint(app: &App, id: StableId) -> JointDef {
    app.world()
        .get::<JointDef>(entity_of(app, id).unwrap())
        .unwrap()
        .clone()
}

fn pose(app: &App, id: StableId) -> PosRot {
    PosRot::from_transform(
        app.world()
            .get::<Transform>(entity_of(app, id).unwrap())
            .unwrap(),
    )
}

/// The two world anchors of a body-to-body joint.
fn anchors(app: &App, def: &JointDef) -> (Vec2, Vec2) {
    let a = pose(app, def.body_a);
    let b = pose(app, def.body_b.unwrap());
    (
        def.anchor_world(a.pos, a.rot),
        b.pos + Vec2::from_angle(b.rot).rotate(def.anchor_b.value()),
    )
}

/// Two boxes hinged where they overlap, the hinge limited and motored, and
/// a thruster on the first box. Sizes and places are dyadic, so the mirror
/// arithmetic is exact.
fn hinged_pair(app: &mut App, rot: f32) -> (StableId, StableId, StableId) {
    let mut left = box_record(Vec2::new(-0.5, 1.0), 1.0, 0.5);
    left.pose.rot = rot;
    let mut right = box_record(Vec2::new(-0.125, 1.25), 0.5, 0.25);
    right.pose.rot = rot;
    // Both anchors on one world point, in each body's frame.
    let point = Vec2::new(-0.25, 1.125);
    let local = |p: &PosRot| Vec2::from_angle(-p.rot).rotate(point - p.pos);
    let (anchor_a, anchor_b) = (local(&left.pose), local(&right.pose));
    let a = spawn(app, left);
    let b = spawn(app, right);
    let hinge = spawn_joint(
        app,
        JointDef {
            kind: JointKind::Hinge {
                limits: Some([Angle(-0.25), Angle(0.5)]),
                motor: Some(AngularMotorDef {
                    target_velocity: AngularVelocity(2.0),
                    ..AngularMotorDef::default()
                }),
            },
            common: JointCommon::default(),
            body_a: a,
            body_b: Some(b),
            anchor_a: Displacement(anchor_a),
            anchor_b: Displacement(anchor_b),
            rest_rot_a: Angle(rot),
            rest_rot_b: Angle(rot),
        },
    );
    (a, b, hinge)
}

#[test]
fn mirroring_a_hinged_pair_twice_gives_the_scene_back() {
    for op in ["body.mirror_h", "body.mirror_v"] {
        let mut app = paused_app();
        let (a, _, hinge) = hinged_pair(&mut app, 0.0);
        invoke(
            &mut app,
            "node.thruster",
            json!({"body": a.0.to_string(), "x": -0.75, "y": 1.0, "angle": 0.5, "thrust": 10}),
        );
        let before = record(&mut app);
        let depth = app.world().resource::<HistoryInfo>().undo_depth;

        invoke(&mut app, op, json!({"targets": [0, 1]}));
        let info = app.world().resource::<HistoryInfo>();
        assert_eq!(info.undo_depth, depth + 1, "{op}: one undo step");
        assert_eq!(info.undo_label, Some("mirror"), "{op}");
        assert_ne!(
            record(&mut app),
            before,
            "{op}: the mirror changed the scene"
        );

        // The hinge still joins the same material point of both bodies.
        let def = joint(&app, hinge);
        let (wa, wb) = anchors(&app, &def);
        assert!(
            (wa - wb).length() < 1e-5,
            "{op}: anchors meet ({wa} vs {wb})"
        );
        // All its bodies turned over, so its relative angles did too.
        let JointKind::Hinge { limits, motor } = def.kind else {
            panic!("still a hinge");
        };
        assert_eq!(limits, Some([Angle(-0.5), Angle(0.25)]), "{op}");
        assert_eq!(
            motor.unwrap().target_velocity,
            AngularVelocity(-2.0),
            "{op}"
        );
        // The thruster rides the mirrored point and pushes the mirrored way.
        let pa = pose(&app, a);
        let (node_kind, offset) = {
            let world = app.world_mut();
            let mut q = world.query::<(&NodeKind, &gradiance::domain::node::NodeAttachment)>();
            let (k, at) = q.iter(world).next().unwrap();
            (k.clone(), at.offset.value())
        };
        let NodeKind::Thruster(t) = node_kind else {
            panic!("a thruster");
        };
        let world_dir = Vec2::from_angle(pa.rot + t.angle.value());
        let want_dir = if op == "body.mirror_h" {
            Vec2::new(-0.5_f32.cos(), 0.5_f32.sin())
        } else {
            Vec2::new(0.5_f32.cos(), -0.5_f32.sin())
        };
        assert!((world_dir - want_dir).length() < 1e-5, "{op}: thrust axis");
        let node_world = pa.pos + Vec2::from_angle(pa.rot).rotate(offset);
        // The pair's bounds are x in [-1, 0.125], y in [0.75, 1.375].
        let want = if op == "body.mirror_h" {
            Vec2::new(-0.125, 1.0)
        } else {
            Vec2::new(-0.75, 1.125)
        };
        assert!((node_world - want).length() < 1e-5, "{op}: {node_world}");

        invoke(&mut app, op, json!({"targets": [0, 1]}));
        let after = record(&mut app);
        assert_eq!(after.bodies, before.bodies, "{op}: bodies back");
        assert_eq!(after.joints, before.joints, "{op}: joints back");
        // The thruster's axis goes through `π − a` twice; its offset is
        // exact.
        assert_eq!(after.nodes.len(), 1);
        assert_eq!(
            after.nodes[0].attachment, before.nodes[0].attachment,
            "{op}: node offset back"
        );

        // Undo walks back one mirror at a time.
        undo(&mut app);
        undo(&mut app);
        assert_eq!(record(&mut app).bodies, before.bodies, "{op}: undone");
    }
}

#[test]
fn a_rotated_pair_mirrors_without_shear_and_keeps_its_hinge() {
    let mut app = paused_app();
    let (a, b, hinge) = hinged_pair(&mut app, 0.5);
    let before = record(&mut app);
    invoke(&mut app, "body.mirror_h", json!({"targets": [0, 1]}));

    // A pure mirror keeps a box a box and turns it to the reflected angle.
    for id in [a, b] {
        let shape = app
            .world()
            .get::<ShapeDef>(entity_of(&app, id).unwrap())
            .unwrap()
            .clone();
        assert!(matches!(shape, ShapeDef::Box { .. }), "{shape:?}");
        assert!((pose(&app, id).rot + 0.5).abs() < 1e-5);
    }
    let def = joint(&app, hinge);
    let (wa, wb) = anchors(&app, &def);
    assert!((wa - wb).length() < 1e-5, "anchors meet ({wa} vs {wb})");
    assert!((def.rest_rot_a.value() + 0.5).abs() < 1e-5);

    invoke(&mut app, "body.mirror_h", json!({"targets": [0, 1]}));
    let after = record(&mut app);
    for (x, y) in after.bodies.iter().zip(&before.bodies) {
        assert_eq!(x.shape, y.shape);
        assert!((x.pose.pos - y.pose.pos).length() < 1e-5);
        assert!((x.pose.rot - y.pose.rot).abs() < 1e-5);
    }
}

#[test]
fn mirroring_one_end_moves_a_world_pin_with_its_body() {
    let mut app = paused_app();
    let body = spawn(&mut app, box_record(Vec2::new(1.0, 2.0), 0.5, 0.5));
    let pin = spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Hinge {
                limits: Some([Angle(-0.25), Angle(0.5)]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: body,
            body_b: None,
            anchor_a: Displacement(Vec2::new(0.25, 0.0)),
            anchor_b: Displacement(Vec2::new(1.25, 2.0)),
            rest_rot_a: Angle(0.0),
            rest_rot_b: Angle(0.0),
        },
    );
    invoke(&mut app, "body.mirror_h", json!({"targets": [0]}));
    let def = joint(&app, pin);
    let p = pose(&app, body);
    assert_eq!(def.anchor_world(p.pos, p.rot), def.anchor_b.value());
    assert_eq!(def.anchor_b.value(), Vec2::new(0.75, 2.0));
    let JointKind::Hinge { limits, .. } = def.kind else {
        panic!("hinge");
    };
    assert_eq!(
        limits,
        Some([Angle(-0.5), Angle(0.25)]),
        "a pinned body turns its limits over"
    );
}

#[test]
fn mirror_is_the_last_operation_but_has_nothing_to_adjust() {
    let mut app = paused_app();
    hinged_pair(&mut app, 0.0);
    invoke(&mut app, "body.mirror_v", json!({"targets": [0, 1]}));
    let last = app
        .world()
        .resource::<LastOp>()
        .0
        .as_ref()
        .map(|r| r.call.id);
    assert_eq!(last, Some("body.mirror_v"));
    // An empty selection mirrors nothing and records nothing.
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    invoke(&mut app, "body.mirror_h", json!({"targets": ["selection"]}));
    assert_eq!(app.world().resource::<HistoryInfo>().undo_depth, depth);
}

/// A box hanging still from a world pin: the reaction force is its weight.
fn hanging(break_force: Option<f32>) -> (App, StableId, StableId) {
    let mut app = paused_app();
    let body = spawn(&mut app, box_record(Vec2::new(0.0, 1.0), 0.4, 0.4));
    let pin = spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Hinge {
                limits: None,
                motor: None,
            },
            common: JointCommon {
                break_force: break_force.map(Force),
                ..JointCommon::default()
            },
            body_a: body,
            body_b: None,
            anchor_a: Displacement(Vec2::new(0.0, 0.2)),
            anchor_b: Displacement(Vec2::new(0.0, 1.2)),
            rest_rot_a: Angle(0.0),
            rest_rot_b: Angle(0.0),
        },
    );
    (app, body, pin)
}

fn run(app: &mut App, frames: usize) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(app, frames);
}

fn load(app: &App, joint: StableId) -> f32 {
    app.world()
        .get::<JointLoad>(entity_of(app, joint).unwrap())
        .unwrap()
        .0
}

#[test]
fn a_joint_at_ninety_percent_of_its_break_force_draws_hot() {
    // Measure the hanging weight on an unbreakable pin first.
    let (mut app, body, pin) = hanging(None);
    run(&mut app, 30);
    let weight = app
        .world()
        .get::<avian2d::prelude::ComputedMass>(entity_of(&app, body).unwrap())
        .unwrap()
        .value()
        * app
            .world()
            .resource::<SimSettings>()
            .gravity
            .value()
            .length();
    // Unbreakable: against ten times the weight it carries.
    let calm = load(&app, pin);
    assert!(
        (calm - 0.1).abs() < 0.01,
        "a hanging pendulum reads a tenth ({calm})"
    );
    let ink = &gradiance::domain::theme::active(None).ink;
    assert_ne!(load_colour(ink.joint, ink.danger, calm), ink.danger);

    // Rated at its weight / 0.9: the pin carries 90 % of what breaks it.
    let (mut app, _, pin) = hanging(Some(weight / 0.9));
    run(&mut app, 30);
    let hot = load(&app, pin);
    assert!(!joint(&app, pin).common.broken, "still holding");
    assert!((hot - 0.9).abs() < 1e-3, "90 % ({hot})");
    let drawn = load_colour(ink.joint, ink.danger, hot);
    let off = (drawn.r - ink.danger.r)
        .abs()
        .max((drawn.g - ink.danger.g).abs())
        .max((drawn.b - ink.danger.b).abs());
    assert!(off < 0.03, "90 % draws in the hot colour ({drawn:?})");
    assert_eq!(load_colour(ink.joint, ink.danger, HOT_LOAD), ink.danger);
    assert_eq!(load_colour(ink.joint, ink.danger, 0.0), ink.joint);

    // Derived: never in the saved scene.
    let saved = gradiance::scene::to_ron(&SceneRecord::capture(app.world_mut())).unwrap();
    assert!(!saved.contains("JointLoad"), "the load is not saved");
}
