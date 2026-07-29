//! Joint tests: native constraint behavior, cascades, and the
//! combinatorial invariants that sank the previous implementation.

// Test-only file: unwraps are the failure mechanism, and the proptest
// body is one deliberate scenario script.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use crate::harness::{body_count, box_record, entity_of, headless_app, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::prelude::*;

fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
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

fn joint_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<Joint>>()
        .iter(app.world())
        .count()
}

fn pose_of(app: &App, id: StableId) -> PosRot {
    let entity = entity_of(app, id).unwrap();
    PosRot::from_transform(app.world().get::<Transform>(entity).unwrap())
}

fn hinge(body_a: StableId, body_b: Option<StableId>, anchor_a: Vec2, anchor_b: Vec2) -> JointDef {
    JointDef {
        kind: JointKind::Hinge {
            limits: None,
            motor: None,
        },
        common: JointCommon::default(),
        body_a,
        body_b,
        anchor_a,
        anchor_b,
        rest_rot_a: 0.0,
        rest_rot_b: 0.0,
    }
}

// ---------- Spike: native motor oscillates between limits ----------

#[test]
fn motorized_hinge_oscillates_between_its_limits() {
    let mut app = headless_app();
    let mut anchor = box_record(Vec2::ZERO, 20.0, 20.0);
    anchor.physics.kind = BodyKind::Static;
    let anchor_id = spawn_body(&mut app, anchor);
    // Arm hinged at its left end to the anchor's center.
    let arm = box_record(Vec2::new(40.0, 0.0), 80.0, 10.0);
    let arm_id = spawn_body(&mut app, arm);

    let limits = [-0.6_f32, 0.6];
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Hinge {
                limits: Some(limits),
                motor: Some(AngularMotorDef {
                    target_velocity: gradiance::units::AngularVelocity(3.0),
                    oscillate: true,
                    ..default()
                }),
            },
            common: JointCommon::default(),
            body_a: anchor_id,
            body_b: Some(arm_id),
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::new(-40.0, 0.0),
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );

    // Track the arm's relative angle over ~6 simulated seconds.
    let arm_entity = entity_of(&app, arm_id).unwrap();
    let mut min_seen = f32::MAX;
    let mut max_seen = f32::MIN;
    let mut direction_changes = 0;
    let mut last_angle = 0.0_f32;
    let mut last_delta = 0.0_f32;
    for _ in 0..360 {
        step(&mut app, 1);
        let angle = PosRot::from_transform(app.world().get::<Transform>(arm_entity).unwrap()).rot;
        let delta = angle - last_angle;
        if delta * last_delta < -1e-6 {
            direction_changes += 1;
        }
        if delta.abs() > 1e-6 {
            last_delta = delta;
        }
        last_angle = angle;
        min_seen = min_seen.min(angle);
        max_seen = max_seen.max(angle);
    }

    assert!(
        direction_changes >= 2,
        "motor must reverse at limits (changes = {direction_changes})"
    );
    let tol = 0.15;
    assert!(
        min_seen > limits[0] - tol && max_seen < limits[1] + tol,
        "angle stayed within limits ({min_seen:.2}..{max_seen:.2})"
    );
    assert!(
        max_seen - min_seen > 0.5,
        "arm actually swept ({min_seen:.2}..{max_seen:.2})"
    );
}

// ---------- World pin: pendulum ----------

#[test]
fn world_pinned_body_swings_about_a_fixed_anchor() {
    let mut app = headless_app();
    let body = box_record(Vec2::new(40.0, 0.0), 80.0, 10.0);
    let body_id = spawn_body(&mut app, body);
    // Pin the rod's left end (world point (0,0)) to the world.
    spawn_joint(
        &mut app,
        hinge(body_id, None, Vec2::new(-40.0, 0.0), Vec2::ZERO),
    );

    step(&mut app, 240); // let it swing under gravity

    let pose = pose_of(&app, body_id);
    let anchor_world = pose.pos + Vec2::from_angle(pose.rot).rotate(Vec2::new(-40.0, 0.0));
    assert!(
        anchor_world.length() < 3.0,
        "pinned point stays at the pin ({anchor_world})"
    );
    assert!(
        pose.rot.abs() > 0.5,
        "rod swung down under gravity (rot = {})",
        pose.rot
    );
    // No pin collider: nothing to collide with, so no explosive contact —
    // the body must still be near the pin, not launched away.
    assert!(pose.pos.length() < 100.0, "no pin explosion ({})", pose.pos);
}

// ---------- Slider ----------

#[test]
fn slider_constrains_motion_to_the_axis_and_respects_limits() {
    let mut app = headless_app();
    let mut rail = box_record(Vec2::ZERO, 20.0, 20.0);
    rail.physics.kind = BodyKind::Static;
    let rail_id = spawn_body(&mut app, rail);
    let cart = box_record(Vec2::new(0.0, -30.0), 20.0, 20.0);
    let cart_id = spawn_body(&mut app, cart);

    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Slider {
                axis: Vec2::Y,
                limits: Some([-120.0, 0.0]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: rail_id,
            body_b: Some(cart_id),
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );

    step(&mut app, 240); // gravity pulls the cart down the rail

    let pose = pose_of(&app, cart_id);
    assert!(
        pose.pos.x.abs() < 2.0,
        "cart stays on the vertical axis (x = {})",
        pose.pos.x
    );
    assert!(
        pose.pos.y < -60.0,
        "cart slid down the rail (y = {})",
        pose.pos.y
    );
    assert!(
        pose.pos.y > -135.0,
        "lower limit holds (y = {})",
        pose.pos.y
    );
}

// ---------- Command cascades ----------

#[test]
fn deleting_a_body_cascades_its_joints_and_undo_restores_both() {
    let mut app = paused_app();
    let a = spawn_body(&mut app, box_record(Vec2::ZERO, 40.0, 20.0));
    let b = spawn_body(&mut app, box_record(Vec2::new(30.0, 0.0), 40.0, 20.0));
    spawn_joint(
        &mut app,
        hinge(a, Some(b), Vec2::new(15.0, 0.0), Vec2::new(-15.0, 0.0)),
    );
    assert_eq!(joint_count(&mut app), 1);

    app.world_mut()
        .write_message(DeleteIntent { targets: vec![a] });
    app.update();
    assert_eq!(body_count(&mut app), 1, "body a gone");
    assert_eq!(joint_count(&mut app), 0, "joint cascaded");

    undo(&mut app);
    assert_eq!(body_count(&mut app), 2);
    assert_eq!(joint_count(&mut app), 1, "undo restored the joint too");
    // The restored joint still resolves both endpoints.
    let mut q = app.world_mut().query::<&JointDef>();
    let def = q.iter(app.world()).next().unwrap();
    assert!(entity_of(&app, def.body_a).is_some());
    assert!(entity_of(&app, def.body_b.unwrap()).is_some());
}

#[test]
fn duplicating_a_hinged_assembly_clones_and_remaps_the_joint() {
    let mut app = paused_app();
    let a = spawn_body(&mut app, box_record(Vec2::ZERO, 40.0, 20.0));
    let b = spawn_body(&mut app, box_record(Vec2::new(30.0, 0.0), 40.0, 20.0));
    spawn_joint(
        &mut app,
        hinge(a, Some(b), Vec2::new(15.0, 0.0), Vec2::new(-15.0, 0.0)),
    );

    app.world_mut().write_message(DuplicateIntent {
        sources: vec![a, b],
        offset: Vec2::new(200.0, 0.0),
    });
    app.update();

    assert_eq!(body_count(&mut app), 4);
    assert_eq!(joint_count(&mut app), 2, "internal joint cloned");
    // The clone's endpoints are the *new* bodies, and they resolve.
    let mut q = app.world_mut().query::<&JointDef>();
    let defs: Vec<JointDef> = q.iter(app.world()).cloned().collect();
    let clone = defs
        .iter()
        .find(|d| d.body_a != a)
        .expect("cloned joint has remapped ids");
    assert_ne!(clone.body_b.unwrap(), b);
    assert!(entity_of(&app, clone.body_a).is_some());
    assert!(entity_of(&app, clone.body_b.unwrap()).is_some());

    undo(&mut app);
    assert_eq!(body_count(&mut app), 2);
    assert_eq!(joint_count(&mut app), 1);
}

#[test]
fn duplicating_a_world_pinned_body_moves_the_pin_anchor() {
    let mut app = paused_app();
    let a = spawn_body(&mut app, box_record(Vec2::ZERO, 40.0, 20.0));
    spawn_joint(&mut app, hinge(a, None, Vec2::ZERO, Vec2::new(5.0, 5.0)));

    app.world_mut().write_message(DuplicateIntent {
        sources: vec![a],
        offset: Vec2::new(100.0, 0.0),
    });
    app.update();

    let mut q = app.world_mut().query::<&JointDef>();
    let defs: Vec<JointDef> = q.iter(app.world()).cloned().collect();
    assert_eq!(defs.len(), 2);
    let clone = defs.iter().find(|d| d.body_a != a).unwrap();
    assert_eq!(
        clone.anchor_b,
        Vec2::new(105.0, 5.0),
        "world anchor translated with the clone"
    );
}

// ---------- Connector tool gestures ----------

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn click(app: &mut App, p: Vec2) {
    // These gesture tests assert exact raw-point anchors; object snapping
    // (which would helpfully pull anchors onto midpoints/centers — the
    // desired interactive behavior) is disabled for determinism.
    app.world_mut().resource_mut::<SnapConfig>().objects_enabled = false;
    set_cursor(app, p);
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    input.press(MouseButton::Left);
    app.update();
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    input.release(MouseButton::Left);
    app.update();
}

#[test]
fn hinge_tool_connects_the_two_topmost_bodies_with_local_anchors() {
    let mut app = paused_app();
    let a = spawn_body(&mut app, box_record(Vec2::ZERO, 60.0, 40.0));
    let b = spawn_body(&mut app, box_record(Vec2::new(40.0, 0.0), 60.0, 40.0));
    app.update();

    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Hinge);
    app.update();

    click(&mut app, Vec2::new(20.0, 0.0)); // inside the overlap

    assert_eq!(joint_count(&mut app), 1);
    let mut q = app.world_mut().query::<&JointDef>();
    let def = q.iter(app.world()).next().unwrap().clone();
    assert!(matches!(def.kind, JointKind::Hinge { .. }));
    let (ia, ib) = (def.body_a, def.body_b.unwrap());
    assert!(
        (ia == a && ib == b) || (ia == b && ib == a),
        "connects the two bodies under the cursor"
    );
    // Anchors express the same world point in each body's local frame.
    let pa = pose_of(&app, def.body_a);
    let world_a = pa.pos + Vec2::from_angle(pa.rot).rotate(def.anchor_a);
    assert!((world_a - Vec2::new(20.0, 0.0)).length() < 1e-3);

    undo(&mut app);
    assert_eq!(joint_count(&mut app), 0, "joint spawn is one undo step");
}

#[test]
fn hinge_tool_pins_a_single_body_to_the_world() {
    let mut app = paused_app();
    spawn_body(&mut app, box_record(Vec2::ZERO, 60.0, 40.0));
    app.update();

    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Hinge);
    app.update();
    click(&mut app, Vec2::new(10.0, 5.0));

    assert_eq!(joint_count(&mut app), 1);
    let mut q = app.world_mut().query::<&JointDef>();
    let def = q.iter(app.world()).next().unwrap().clone();
    assert_eq!(def.body_b, None, "single body → world pin");
    assert_eq!(def.anchor_b, Vec2::new(10.0, 5.0), "world anchor point");
}

// ---------- Combinatorial invariant (the old project's blind spot) ----------

/// After ANY sequence of commands + undo/redo, no joint may reference a
/// missing body, and undoing everything must empty the world.
#[test]
fn random_command_sequences_never_leave_dangling_joints() {
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        SpawnBody(u8),
        HingePair(u8, u8),
        PinBody(u8),
        DeleteBody(u8),
        Duplicate(u8),
        Undo,
        Redo,
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u8..8).prop_map(Op::SpawnBody),
            (0u8..8, 0u8..8).prop_map(|(a, b)| Op::HingePair(a, b)),
            (0u8..8).prop_map(Op::PinBody),
            (0u8..8).prop_map(Op::DeleteBody),
            (0u8..8).prop_map(Op::Duplicate),
            Just(Op::Undo),
            Just(Op::Redo),
        ]
    }

    let mut runner = proptest::test_runner::TestRunner::new(proptest::test_runner::Config {
        cases: 24,
        ..Default::default()
    });

    runner
        .run(&proptest::collection::vec(op_strategy(), 1..24), |ops| {
            let mut app = paused_app();
            let mut slots: Vec<Option<StableId>> = vec![None; 8];

            for op in ops {
                match op {
                    Op::SpawnBody(slot) => {
                        let record = box_record(Vec2::new(f32::from(slot) * 50.0, 0.0), 40.0, 20.0);
                        slots[slot as usize] = Some(record.id);
                        app.world_mut().write_message(SpawnBodyIntent { record });
                    }
                    Op::HingePair(sa, sb) => {
                        if let (Some(a), Some(b)) = (slots[sa as usize], slots[sb as usize])
                            && a != b
                        {
                            let record = JointRecord {
                                id: StableId::new(),
                                def: JointDef {
                                    kind: JointKind::Hinge {
                                        limits: None,
                                        motor: None,
                                    },
                                    common: JointCommon::default(),
                                    body_a: a,
                                    body_b: Some(b),
                                    anchor_a: Vec2::ZERO,
                                    anchor_b: Vec2::ZERO,
                                    rest_rot_a: 0.0,
                                    rest_rot_b: 0.0,
                                },
                            };
                            app.world_mut().write_message(SpawnJointIntent { record });
                        }
                    }
                    Op::PinBody(slot) => {
                        if let Some(a) = slots[slot as usize] {
                            let record = JointRecord {
                                id: StableId::new(),
                                def: JointDef {
                                    kind: JointKind::Hinge {
                                        limits: None,
                                        motor: None,
                                    },
                                    common: JointCommon::default(),
                                    body_a: a,
                                    body_b: None,
                                    anchor_a: Vec2::ZERO,
                                    anchor_b: Vec2::ZERO,
                                    rest_rot_a: 0.0,
                                    rest_rot_b: 0.0,
                                },
                            };
                            app.world_mut().write_message(SpawnJointIntent { record });
                        }
                    }
                    Op::DeleteBody(slot) => {
                        if let Some(a) = slots[slot as usize] {
                            app.world_mut()
                                .write_message(DeleteIntent { targets: vec![a] });
                        }
                    }
                    Op::Duplicate(slot) => {
                        if let Some(a) = slots[slot as usize] {
                            app.world_mut().write_message(DuplicateIntent {
                                sources: vec![a],
                                offset: Vec2::new(10.0, 10.0),
                            });
                        }
                    }
                    Op::Undo => {
                        app.world_mut().write_message(UndoIntent);
                    }
                    Op::Redo => {
                        app.world_mut().write_message(RedoIntent);
                    }
                }
                app.update();

                // INVARIANT: every live joint resolves every endpoint.
                let defs: Vec<JointDef> = {
                    let mut q = app.world_mut().query::<&JointDef>();
                    q.iter(app.world()).cloned().collect()
                };
                for def in &defs {
                    for id in def.referenced_bodies() {
                        prop_assert!(
                            entity_of(&app, id).is_some(),
                            "dangling joint endpoint after {op:?}"
                        );
                    }
                }
            }

            // Undo everything → world must be empty again.
            for _ in 0..64 {
                app.world_mut().write_message(UndoIntent);
                app.update();
            }
            prop_assert_eq!(body_count(&mut app), 0);
            prop_assert_eq!(joint_count(&mut app), 0);
            Ok(())
        })
        .unwrap();
}

// ---------- Hinge freedom (the weld tool no longer makes joints) ----------

/// A dynamic arm hanging off a static block swings down under gravity —
/// the hinge grants relative rotation. (Rigid links are the weld tool's
/// *merge* now, covered by the CSG merge tests and the weld-tool tests.)
#[test]
fn hinged_arm_swings_down_under_gravity() {
    let mut app = headless_app();
    let mut anchor_block = box_record(Vec2::ZERO, 20.0, 20.0);
    anchor_block.physics.kind = BodyKind::Static;
    let block = spawn_body(&mut app, anchor_block);
    // Horizontal arm extending to the right of the block.
    let arm = spawn_body(&mut app, box_record(Vec2::new(40.0, 0.0), 60.0, 8.0));

    let def = hinge(
        block,
        Some(arm),
        Vec2::new(10.0, 0.0),
        Vec2::new(-30.0, 0.0),
    );
    spawn_joint(&mut app, def);
    step(&mut app, 240);

    let rot = pose_of(&app, arm).rot.abs();
    assert!(
        rot > 0.4,
        "hinged arm must swing down under gravity (rot {rot})"
    );
}

/// A hinge with angle limits but **no motor** must stop the free arm at the
/// limit under gravity — the reported "limits have no effect" case.
#[test]
fn hinge_angle_limit_stops_a_free_arm_under_gravity() {
    let mut app = headless_app();
    let mut anchor_block = box_record(Vec2::ZERO, 20.0, 20.0);
    anchor_block.physics.kind = BodyKind::Static;
    let block = spawn_body(&mut app, anchor_block);
    let arm = spawn_body(&mut app, box_record(Vec2::new(40.0, 0.0), 60.0, 8.0));

    let limits = [-0.3_f32, 0.3];
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Hinge {
                limits: Some(limits),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: block,
            body_b: Some(arm),
            anchor_a: Vec2::new(10.0, 0.0),
            anchor_b: Vec2::new(-30.0, 0.0),
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );
    step(&mut app, 240);

    let rot = pose_of(&app, arm).rot;
    assert!(
        rot < -0.05,
        "arm swung toward the lower limit under gravity (rot {rot})"
    );
    assert!(
        rot >= limits[0] - 0.15,
        "angle limit holds — arm didn't blow past it (rot {rot}, min {})",
        limits[0]
    );
}

/// A **world-pinned** hinge with angle limits holds the body's swing at the
/// fixed world limit (the pin's static frame). This is the reference case the
/// limit gizmo must anchor to the world, not to the body's live rotation.
#[test]
fn world_pin_hinge_limit_holds_the_swing() {
    let mut app = headless_app();
    let rod = box_record(Vec2::new(40.0, 0.0), 80.0, 10.0);
    let rod_id = spawn_body(&mut app, rod);
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Hinge {
                limits: Some([-0.3, 0.3]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: rod_id,
            body_b: None,
            anchor_a: Vec2::new(-40.0, 0.0),
            anchor_b: Vec2::ZERO,
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );
    step(&mut app, 240);

    let rot = pose_of(&app, rod_id).rot;
    assert!(
        rot < -0.1,
        "rod swung down toward its lower limit (rot {rot})"
    );
    assert!(
        rot >= -0.3 - 0.15,
        "the world-pin angle limit held the swing (rot {rot})"
    );
}

// ---------- Rest-orientation frames (user snapshot regressions) ----------

/// The user's snapshot gradiance-1783344618.ron distilled: a rotated body
/// on a world-pin slider. With identity joint frames the solver snaps the
/// body upright and it jitters and explodes; rest-rotation frames must
/// keep it stable, rotation-locked at its authored angle, and sliding
/// only along the axis.
#[test]
fn rotated_world_pin_slider_stays_stable_and_rotation_locked() {
    let mut app = headless_app();
    let rot = -1.254;
    let start = Vec2::new(-31.9, 259.6);
    let mut record = box_record(start, 79.0, 44.9);
    record.pose.rot = rot;
    let body = spawn_body(&mut app, record);
    let axis = Vec2::new(-0.774_418_2, -0.632_674_04);
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Slider {
                axis,
                limits: None,
                motor: None,
            },
            common: JointCommon::default(),
            body_a: body,
            body_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: start,
            rest_rot_a: rot,
            rest_rot_b: 0.0,
        },
    );
    step(&mut app, 300);

    // With no limits and no friction the body slides freely downhill —
    // far, but finitely and smoothly. Explosion = NaN or absurd distance.
    let pose = pose_of(&app, body);
    assert!(
        pose.pos.is_finite() && pose.pos.length() < 100_000.0,
        "no explosion (at {})",
        pose.pos
    );
    assert!(
        (pose.rot - rot).abs() < 0.05,
        "slider locks rotation at its rest angle (rot {})",
        pose.rot
    );
    // Whatever gravity did, motion stays on the slider line.
    let world_axis = Vec2::from_angle(rot).rotate(axis);
    let disp = pose.pos - start;
    if disp.length() > 1.0 {
        assert!(
            disp.normalize().perp_dot(world_axis).abs() < 0.05,
            "displacement {disp} leaves the slider axis"
        );
    }
}

fn strut(body_a: StableId, body_b: Option<StableId>, rest_length: f32, stiffness: f32) -> JointDef {
    JointDef {
        kind: JointKind::Spring {
            rest_length,
            stiffness,
            damping: 5.0,
            range: None,
        },
        common: JointCommon::default(),
        body_a,
        body_b,
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        rest_rot_a: 0.0,
        rest_rot_b: 0.0,
    }
}

#[test]
fn strut_derives_a_distance_joint_and_damping() {
    use avian2d::prelude::{DistanceJoint, JointDamping};
    let mut app = headless_app();
    let a = spawn_body(&mut app, box_record(Vec2::ZERO, 20.0, 20.0));
    let b = spawn_body(&mut app, box_record(Vec2::new(60.0, 0.0), 20.0, 20.0));
    let jid = spawn_joint(&mut app, strut(a, Some(b), 60.0, 500.0));
    step(&mut app, 2);

    let je = entity_of(&app, jid).unwrap();
    assert!(
        app.world().get::<DistanceJoint>(je).is_some(),
        "strut derives a DistanceJoint"
    );
    let damping = app.world().get::<JointDamping>(je).unwrap();
    assert!(
        (damping.linear - 5.0).abs() < 1e-3,
        "damping maps onto JointDamping ({})",
        damping.linear
    );
}

#[test]
fn strut_pulls_a_body_toward_its_rest_length() {
    let mut app = headless_app();
    // A static anchor at the origin.
    let mut anchor = box_record(Vec2::ZERO, 20.0, 20.0);
    anchor.physics.kind = BodyKind::Static;
    let anchor_id = spawn_body(&mut app, anchor);
    // A dynamic body 100 px away, gravity off so only the strut acts.
    let mut ball = box_record(Vec2::new(100.0, 0.0), 20.0, 20.0);
    ball.physics.gravity_scale = 0.0;
    let ball_id = spawn_body(&mut app, ball);
    // A stiff strut with a 50 px rest length between their centres.
    spawn_joint(&mut app, strut(anchor_id, Some(ball_id), 50.0, 1000.0));

    let start = pose_of(&app, ball_id).pos.x;
    assert!(
        (start - 100.0).abs() < 1.0,
        "ball starts at x = 100 ({start})"
    );
    step(&mut app, 180);
    let end = pose_of(&app, ball_id).pos.x;
    assert!(end < 90.0, "the strut pulled the ball inward (x = {end})");
    assert!(
        (end - 50.0).abs() < 20.0,
        "the ball settles near the 50 px rest length (x = {end})"
    );
}

#[test]
fn a_strut_is_undoable_and_persists_its_kind() {
    let mut app = paused_app();
    let a = spawn_body(&mut app, box_record(Vec2::ZERO, 20.0, 20.0));
    let b = spawn_body(&mut app, box_record(Vec2::new(80.0, 0.0), 20.0, 20.0));
    spawn_joint(&mut app, strut(a, Some(b), 80.0, 250.0));
    assert_eq!(joint_count(&mut app), 1);
    undo(&mut app);
    assert_eq!(
        joint_count(&mut app),
        0,
        "a scripted strut is one undoable command"
    );
}

/// A prismatic joint must never grant relative rotation — under a hard
/// torque load the connected body keeps its rest angle (feedback: "if I
/// wanted it to rotate I'd attach it with a hinge").
#[test]
fn prismatic_locks_rotation_under_torque_load() {
    let mut app = headless_app();
    let mut base = box_record(Vec2::ZERO, 40.0, 40.0);
    base.physics.kind = BodyKind::Static;
    let a = spawn_body(&mut app, base);
    // A long arm on a horizontal slider: gravity on the off-axis mass is a
    // steady torque about the anchor.
    let arm = spawn_body(&mut app, box_record(Vec2::new(120.0, 0.0), 200.0, 10.0));
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Slider {
                axis: Vec2::X,
                limits: Some([0.0, 100.0]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: a,
            body_b: Some(arm),
            anchor_a: Vec2::new(20.0, 0.0),
            anchor_b: Vec2::new(-100.0, 0.0),
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );
    // Kick it with spin as well: the constraint must absorb it.
    {
        let entity = entity_of(&app, arm).unwrap();
        let mut angular = app
            .world_mut()
            .get_mut::<avian2d::prelude::AngularVelocity>(entity)
            .unwrap();
        angular.0 = 5.0;
    }
    step(&mut app, 300);

    let rot = pose_of(&app, arm).rot;
    assert!(
        rot.abs() < 0.05,
        "prismatic-jointed arm must not rotate (rot {rot})"
    );
}

// ---------- World-pin oscillation with a non-zero rest basis ----------

/// Oscillating motor on a **world pin authored at a tilt**, pinned at the
/// rod's centre so gravity exerts no torque about the pivot. This exercises
/// the rest-basis term of the reversal frame that the zero-basis body-body
/// test above misses: with the pre-audit code the reversal angle omitted the
/// basis, so it lined up with only one bound and the motor stalled into the
/// other instead of sweeping back and forth.
#[test]
fn world_pinned_tilted_motor_oscillates() {
    let mut app = headless_app();
    let mut rod = box_record(Vec2::new(40.0, 0.0), 80.0, 10.0);
    rod.pose.rot = 0.3; // authored tilt => non-zero rest basis
    let rod_id = spawn_body(&mut app, rod);
    let limits = [-0.5_f32, 0.5];
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Hinge {
                limits: Some(limits),
                motor: Some(AngularMotorDef {
                    target_velocity: gradiance::units::AngularVelocity(3.0),
                    oscillate: true,
                    ..default()
                }),
            },
            common: JointCommon::default(),
            body_a: rod_id,
            body_b: None,
            anchor_a: Vec2::ZERO,           // rod centre (local)
            anchor_b: Vec2::new(40.0, 0.0), // world point == rod centre
            rest_rot_a: 0.3,
            rest_rot_b: 0.0,
        },
    );

    let rod_entity = entity_of(&app, rod_id).unwrap();
    let mut direction_changes = 0;
    let (mut last_angle, mut last_delta) = (0.3_f32, 0.0_f32);
    let (mut min_seen, mut max_seen) = (f32::MAX, f32::MIN);
    for _ in 0..360 {
        step(&mut app, 1);
        let angle = PosRot::from_transform(app.world().get::<Transform>(rod_entity).unwrap()).rot;
        let delta = angle - last_angle;
        if delta * last_delta < -1e-6 {
            direction_changes += 1;
        }
        if delta.abs() > 1e-6 {
            last_delta = delta;
        }
        last_angle = angle;
        min_seen = min_seen.min(angle);
        max_seen = max_seen.max(angle);
    }

    assert!(
        direction_changes >= 2,
        "tilted world-pin motor must reverse at both bounds (changes = {direction_changes}, \
         swept {min_seen:.2}..{max_seen:.2})"
    );
    assert!(
        max_seen - min_seen > 0.5,
        "rod actually swept a range ({min_seen:.2}..{max_seen:.2})"
    );
}

/// A continuous motor must not shove the hinge pivot off its pin: the point
/// constraint is rigid, so the driven arm's anchored end stays coincident with
/// the fixed anchor. (With the old fixed 1e7 ceiling the engagement impulse
/// spiked above what the point constraint could absorb in a substep and the
/// pivot drifted; the auto, inertia-scaled ceiling keeps the impulse bounded.)
#[test]
fn motorized_hinge_holds_its_pivot() {
    let mut app = headless_app();
    let mut anchor = box_record(Vec2::ZERO, 20.0, 20.0);
    anchor.physics.kind = BodyKind::Static;
    let anchor_id = spawn_body(&mut app, anchor);
    let arm = box_record(Vec2::new(40.0, 0.0), 80.0, 10.0);
    let arm_id = spawn_body(&mut app, arm);
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Hinge {
                limits: None,
                motor: Some(AngularMotorDef {
                    target_velocity: gradiance::units::AngularVelocity(6.0), // auto ceiling (max_torque = 0)
                    ..default()
                }),
            },
            common: JointCommon::default(),
            body_a: anchor_id,
            body_b: Some(arm_id),
            anchor_a: Vec2::ZERO, // anchor-body centre == world origin
            anchor_b: Vec2::new(-40.0, 0.0), // arm's left end (local)
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );
    step(&mut app, 240);
    // The arm's anchored end must still sit on the pin at the world origin.
    let arm_pose = pose_of(&app, arm_id);
    let anchored_end = arm_pose.pos + Vec2::from_angle(arm_pose.rot).rotate(Vec2::new(-40.0, 0.0));
    assert!(
        anchored_end.length() < 1.0,
        "pivot drifted to {anchored_end:?} (len {})",
        anchored_end.length()
    );
    // And the motor actually drove the arm (it isn't just stuck).
    let spun = angular_velocity(&app, arm_id).abs();
    assert!(spun > 0.5, "motor should spin the arm (w = {spun})");
}

/// Reads a body's avian angular velocity (rad/s), or 0 before the solver runs.
fn angular_velocity(app: &App, id: StableId) -> f32 {
    entity_of(app, id)
        .and_then(|e| app.world().get::<avian2d::prelude::AngularVelocity>(e))
        .map_or(0.0, |w| w.0)
}

/// A strut's stiffness must be scaled for SI: a body hung from a world-pinned
/// spring sags only ~0.1 m (`sag = m·g / k` with `k ≈ 100·m`), not the ~100 m
/// the pre-audit `0.1 N/m` fallback gave. Undamped, it oscillates about that
/// equilibrium, so a generous < 1 m bound distinguishes the two without needing
/// the spring to settle.
#[test]
fn strut_stiffness_keeps_a_hung_body_from_drooping() {
    let mut app = headless_app();
    // A 10x10 body (area 100 => mass 100 at unit density) hanging below a
    // world anchor at the origin; the strut spans the 50-unit gap, relaxed.
    let body = box_record(Vec2::new(0.0, -50.0), 10.0, 10.0);
    let body_id = spawn_body(&mut app, body);
    let stiffness = 100.0 * (10.0 * 10.0); // SPRING_STIFFNESS_PER_MASS * mass
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Spring {
                rest_length: 50.0,
                stiffness,
                damping: 0.0,
                range: None,
            },
            common: JointCommon::default(),
            body_a: body_id,
            body_b: None,
            anchor_a: Vec2::ZERO, // body centre
            anchor_b: Vec2::ZERO, // world anchor
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );

    let mut max_droop = 0.0_f32;
    for _ in 0..240 {
        step(&mut app, 1);
        // How far the body fell below its rest position (-50).
        let droop = -50.0 - pose_of(&app, body_id).pos.y;
        max_droop = max_droop.max(droop);
    }
    assert!(
        max_droop < 1.0,
        "SI stiffness must hold the body up (drooped {max_droop:.2} m; the old \
         0.1 N/m fallback would droop ~100 m)"
    );
    assert!(
        max_droop > 1e-3,
        "the strut is a spring, not rigid (droop {max_droop})"
    );
}

/// A slider (prismatic) motor with an auto ceiling drives its body along the
/// axis — covers the `linear_motor` path, whose force cap now scales with the
/// body's real mass (`motor_ceiling`) rather than a fixed default.
#[test]
fn motorized_slider_drives_body_along_its_axis() {
    let mut app = headless_app();
    let mut anchor = box_record(Vec2::ZERO, 20.0, 20.0);
    anchor.physics.kind = BodyKind::Static;
    let anchor_id = spawn_body(&mut app, anchor);
    let slider = box_record(Vec2::new(0.0, 0.0), 20.0, 20.0);
    let slider_id = spawn_body(&mut app, slider);
    spawn_joint(
        &mut app,
        JointDef {
            kind: JointKind::Slider {
                axis: Vec2::X,
                limits: Some([0.0, 200.0]),
                motor: Some(LinearMotorDef {
                    target_velocity: gradiance::units::Velocity(30.0), // along +X; auto ceiling (max_force = 0)
                    ..default()
                }),
            },
            common: JointCommon::default(),
            body_a: anchor_id,
            body_b: Some(slider_id),
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    );
    let start_x = pose_of(&app, slider_id).pos.x;
    step(&mut app, 120);
    let moved = pose_of(&app, slider_id).pos.x - start_x;
    assert!(
        moved > 10.0,
        "auto-ceiling slider motor must drive the body along +X (moved {moved:.2})"
    );
    // And it stays on the axis (no vertical wander from the constraint).
    assert!(
        pose_of(&app, slider_id).pos.y.abs() < 1.0,
        "slider stays on its axis (y = {})",
        pose_of(&app, slider_id).pos.y
    );
}
