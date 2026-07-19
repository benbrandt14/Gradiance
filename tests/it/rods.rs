//! Rigid-rod tests: the composite spawn command, its undo/redo identity
//! stability, persistence, and duplicate cascade.

#![allow(clippy::unwrap_used)]

use crate::harness::{body_count, box_record, entity_of, paused_app, redo, undo};
use avian2d::prelude::{Collider, CollisionLayers};
use bevy::prelude::*;
use gradiance::command::intent::SpawnRodIntent;
use gradiance::command::rod_cmd::RodSpec;
use gradiance::persist::{from_ron, to_ron};
use gradiance::prelude::*;

/// A rigid-rod spec between `a` and `b`, attached to `hits` (the bodies
/// under each end, if any) — mirrors what the strut tool commits.
fn rod_spec(a: Vec2, b: Vec2, hit_a: Option<StableId>, hit_b: Option<StableId>) -> RodSpec {
    let length = a.distance(b);
    let rot = (b - a).to_angle();
    let rod_id = StableId::new();
    let body = BodyRecord {
        id: rod_id,
        pose: PosRot {
            pos: (a + b) / 2.0,
            rot,
        },
        shape: ShapeDef::Capsule {
            half_length: length / 2.0,
            radius: 2.5,
        },
        physics: BodyPhysics::default(),
        appearance: Appearance::default(),
        depth: DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
        rod: Some(Rod {
            end_a: RodEndKind::Hinge,
            end_b: RodEndKind::Fixed,
        }),
        rod_tip: false,
    };
    let joints = [
        (
            hit_a,
            Vec2::new(-length / 2.0, 0.0),
            JointKind::Hinge {
                limits: None,
                motor: None,
            },
        ),
        (hit_b, Vec2::new(length / 2.0, 0.0), JointKind::Weld),
    ]
    .into_iter()
    .filter_map(|(hit, anchor, kind)| {
        let target = hit?;
        Some(JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind,
                common: JointCommon::default(),
                body_a: rod_id,
                body_b: Some(target),
                anchor_a: anchor,
                anchor_b: Vec2::ZERO,
                rest_rot_a: rot,
                rest_rot_b: 0.0,
            },
        })
    })
    .collect();
    RodSpec {
        bodies: vec![body],
        joints,
    }
}

fn joint_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<Joint>>()
        .iter(app.world())
        .count()
}

#[test]
fn rod_spawn_authors_a_capsule_body_with_collider_and_two_joints() {
    let mut app = paused_app();
    let left = box_record(Vec2::new(-100.0, 0.0), 40.0, 40.0);
    let right = box_record(Vec2::new(100.0, 0.0), 40.0, 40.0);
    let (left_id, right_id) = (left.id, right.id);
    app.world_mut()
        .write_message(SpawnBodyIntent { record: left });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: right });
    app.update();

    let spec = rod_spec(
        Vec2::new(-80.0, 0.0),
        Vec2::new(80.0, 0.0),
        Some(left_id),
        Some(right_id),
    );
    let rod_id = spec.bodies[0].id;
    app.world_mut().write_message(SpawnRodIntent { spec });
    app.update();
    app.update(); // PostUpdate sync derives collider/layers

    assert_eq!(body_count(&mut app), 3);
    assert_eq!(joint_count(&mut app), 2);
    let rod = entity_of(&app, rod_id).unwrap();
    assert!(
        app.world().get::<Collider>(rod).is_some(),
        "capsule collider derived"
    );
    assert!(
        app.world().get::<CollisionLayers>(rod).is_some(),
        "depth layers derived"
    );
    assert!(app.world().get::<Rod>(rod).is_some(), "rod marker present");
}

#[test]
fn rod_undo_redo_is_atomic_and_id_stable() {
    let mut app = paused_app();
    let base = box_record(Vec2::ZERO, 40.0, 40.0);
    let base_id = base.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: base });
    app.update();

    let spec = rod_spec(
        Vec2::new(0.0, 0.0),
        Vec2::new(80.0, 0.0),
        Some(base_id),
        None,
    );
    let rod_id = spec.bodies[0].id;
    let joint_id = spec.joints[0].id;
    app.world_mut().write_message(SpawnRodIntent { spec });
    app.update();
    assert_eq!(body_count(&mut app), 2);
    assert_eq!(joint_count(&mut app), 1);

    undo(&mut app);
    assert_eq!(body_count(&mut app), 1, "undo removes the rod body");
    assert_eq!(joint_count(&mut app), 0, "undo removes the end joint");
    assert!(entity_of(&app, rod_id).is_none());

    redo(&mut app);
    assert_eq!(body_count(&mut app), 2);
    assert_eq!(joint_count(&mut app), 1);
    assert!(entity_of(&app, rod_id).is_some(), "redo restores the id");
    assert!(entity_of(&app, joint_id).is_some());
}

#[test]
fn rod_marker_round_trips_through_ron() {
    let spec = rod_spec(Vec2::ZERO, Vec2::new(60.0, 0.0), None, None);
    let scene = SceneRecord {
        version: gradiance::persist::FORMAT_VERSION,
        app_version: String::new(),
        bodies: spec.bodies,
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    let parsed = from_ron(&text).unwrap();
    assert_eq!(
        parsed.bodies[0].rod,
        Some(Rod {
            end_a: RodEndKind::Hinge,
            end_b: RodEndKind::Fixed,
        })
    );
    assert!(matches!(parsed.bodies[0].shape, ShapeDef::Capsule { .. }));
}

#[test]
fn duplicating_a_rod_assembly_clones_its_joints() {
    let mut app = paused_app();
    let base = box_record(Vec2::ZERO, 40.0, 40.0);
    let base_id = base.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: base });
    app.update();
    let spec = rod_spec(
        Vec2::new(0.0, 0.0),
        Vec2::new(80.0, 0.0),
        Some(base_id),
        None,
    );
    let rod_id = spec.bodies[0].id;
    app.world_mut().write_message(SpawnRodIntent { spec });
    app.update();

    // Duplicate the whole assembly: the internal joint must clone too.
    app.world_mut().write_message(DuplicateIntent {
        sources: vec![base_id, rod_id],
        offset: Vec2::new(0.0, 200.0),
    });
    app.update();
    assert_eq!(body_count(&mut app), 4);
    assert_eq!(joint_count(&mut app), 2, "internal joint cloned");
}

// ---------- Flexure (elastica) rods ----------

/// A flexure spec between two existing bodies (anchors at their centers).
fn flexure_spec(id_a: StableId, pos_a: Vec2, id_b: StableId, pos_b: Vec2, length: f32) -> RodSpec {
    let chord_angle = (pos_b - pos_a).to_angle();
    RodSpec {
        bodies: vec![],
        joints: vec![JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Elastica {
                    length,
                    young_modulus: 1.0e7,
                    thickness: 5.0,
                    damping_ratio: 0.3,
                    end_a: RodEndKind::Hinge,
                    end_b: RodEndKind::Hinge,
                    tangent_a: chord_angle,
                    tangent_b: chord_angle,
                },
                common: JointCommon::default(),
                body_a: id_a,
                body_b: Some(id_b),
                anchor_a: Vec2::ZERO,
                anchor_b: Vec2::ZERO,
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        }],
    }
}

/// Zero-gravity playing app: elastica forces isolated from weight.
fn weightless_app() -> App {
    let mut app = crate::harness::headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();
    app
}

#[test]
fn stretched_flexure_pulls_its_ends_together_and_conserves_momentum() {
    let mut app = weightless_app();
    let a = box_record(Vec2::new(0.0, 0.0), 30.0, 30.0);
    let b = box_record(Vec2::new(210.0, 0.0), 30.0, 30.0);
    let (id_a, id_b) = (a.id, b.id);
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();
    // Rest length 200 but the chord is 210: the rod is stretched.
    app.world_mut().write_message(SpawnRodIntent {
        spec: flexure_spec(id_a, Vec2::ZERO, id_b, Vec2::new(210.0, 0.0), 200.0),
    });
    // Sample within the first quarter of the axial oscillation period so
    // the velocities still point in the restoring direction (the first
    // frame only spawns; forces begin once the cache is derived).
    crate::harness::step(&mut app, 3);

    let va = velocity_of(&mut app, id_a);
    let vb = velocity_of(&mut app, id_b);
    assert!(va.x > 0.1, "A pulled toward B (va = {va})");
    assert!(vb.x < -0.1, "B pulled toward A (vb = {vb})");
    let momentum = va + vb; // equal masses
    assert!(
        momentum.length() < 0.05 * va.length().max(vb.length()),
        "momentum conserved (sum {momentum})"
    );
}

#[test]
fn compressed_flexure_buckles_and_pushes_its_ends_apart() {
    let mut app = weightless_app();
    let a = box_record(Vec2::new(0.0, 0.0), 30.0, 30.0);
    let b = box_record(Vec2::new(190.0, 0.0), 30.0, 30.0);
    let (id_a, id_b) = (a.id, b.id);
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();
    // Rest length 200, chord 190: 5% compressed — far past buckling onset.
    app.world_mut().write_message(SpawnRodIntent {
        spec: flexure_spec(id_a, Vec2::ZERO, id_b, Vec2::new(190.0, 0.0), 200.0),
    });
    // Early frames: the compressive load pushes the ends apart (sampled
    // before the ends rebound through the rest length).
    crate::harness::step(&mut app, 3);
    let vb = velocity_of(&mut app, id_b);
    assert!(vb.x > 0.05, "compression pushes B outward (vb = {vb})");
    // Then let the pitchfork develop: the warm-start cache carries a
    // transverse (buckled) deflection.
    crate::harness::step(&mut app, 30);
    let state = elastica_cache(&mut app);
    let bow = state.x[2].abs().max(state.x[3].abs());
    assert!(bow > 0.5, "transverse buckling amplitude developed ({bow})");
}

#[test]
fn flexure_simulation_is_deterministic() {
    let run = || {
        let mut app = weightless_app();
        let a = box_record(Vec2::new(0.0, 0.0), 30.0, 30.0);
        let b = box_record(Vec2::new(185.0, 20.0), 30.0, 30.0);
        let (id_a, id_b) = (a.id, b.id);
        app.world_mut().write_message(SpawnBodyIntent { record: a });
        app.world_mut().write_message(SpawnBodyIntent { record: b });
        app.update();
        app.world_mut().write_message(SpawnRodIntent {
            spec: flexure_spec(id_a, Vec2::ZERO, id_b, Vec2::new(185.0, 20.0), 200.0),
        });
        crate::harness::step(&mut app, 60);
        let ea = entity_of(&app, id_a).unwrap();
        let eb = entity_of(&app, id_b).unwrap();
        (
            *app.world().get::<Transform>(ea).unwrap(),
            *app.world().get::<Transform>(eb).unwrap(),
        )
    };
    assert_eq!(run(), run(), "bit-identical across two runs");
}

#[test]
fn deleting_an_attached_body_disables_the_flexure_without_panic() {
    let mut app = weightless_app();
    let a = box_record(Vec2::new(0.0, 0.0), 30.0, 30.0);
    let b = box_record(Vec2::new(200.0, 0.0), 30.0, 30.0);
    let (id_a, id_b) = (a.id, b.id);
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();
    app.world_mut().write_message(SpawnRodIntent {
        spec: flexure_spec(id_a, Vec2::ZERO, id_b, Vec2::new(200.0, 0.0), 200.0),
    });
    crate::harness::step(&mut app, 3);
    app.world_mut().write_message(DeleteIntent {
        targets: vec![id_b],
    });
    crate::harness::step(&mut app, 10);
    assert_eq!(body_count(&mut app), 1);
}

/// Zeroes both bodies' velocities (used to discard the startup contact
/// kick from the pre-joint frame, isolating steady-state behavior).
fn reset_velocities(app: &mut App, ids: &[StableId]) {
    for &id in ids {
        let entity = entity_of(app, id).unwrap();
        if let Some(mut v) = app
            .world_mut()
            .get_mut::<avian2d::prelude::LinearVelocity>(entity)
        {
            v.0 = Vec2::ZERO;
        }
        if let Some(mut w) = app
            .world_mut()
            .get_mut::<avian2d::prelude::AngularVelocity>(entity)
        {
            w.0 = 0.0;
        }
    }
}

fn velocity_of(app: &mut App, id: StableId) -> Vec2 {
    let entity = entity_of(app, id).unwrap();
    app.world()
        .get::<avian2d::prelude::LinearVelocity>(entity)
        .map(|v| v.0)
        .unwrap_or_default()
}

fn elastica_cache(app: &mut App) -> gradiance::geometry::elastica::ElasticaState {
    app.world_mut()
        .query::<&gradiance::physics::elastica::ElasticaCache>()
        .single(app.world())
        .map(|c| c.state)
        .unwrap_or_default()
}

// ---------- Boundary-tolerant attachment (revision R2) ----------

fn set_cursor(app: &mut App, p: Vec2) {
    use gradiance::interaction::cursor::CursorWorldPos;
    use gradiance::interaction::snap::SnappedCursor;
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn mouse(app: &mut App, button: MouseButton, down: bool) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    if down {
        input.press(button);
    } else {
        input.release(button);
    }
}

#[test]
fn rod_drawn_tip_to_tip_onto_another_rod_attaches() {
    use gradiance::core::states::ToolState;
    let mut app = paused_app();
    // First rod: capsule from (0,0) to (100,0), spawned directly.
    let spec = rod_spec(Vec2::ZERO, Vec2::new(100.0, 0.0), None, None);
    app.world_mut().write_message(SpawnRodIntent { spec });
    crate::harness::step(&mut app, 2); // colliders derive

    // Draw a second rod starting exactly ON the first rod's tip.
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Strut);
    app.update();
    set_cursor(&mut app, Vec2::new(100.0, 0.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(200.0, 60.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(body_count(&mut app), 2, "second rod authored");
    assert_eq!(
        joint_count(&mut app),
        1,
        "tip-to-tip landing creates the pin joint"
    );
}

#[test]
fn rod_ending_exactly_on_a_box_edge_attaches() {
    use gradiance::core::states::ToolState;
    let mut app = paused_app();
    // Box centered at (150, 0), 40 wide: left edge exactly at x = 130.
    let boxy = box_record(Vec2::new(150.0, 0.0), 40.0, 40.0);
    app.world_mut()
        .write_message(SpawnBodyIntent { record: boxy });
    crate::harness::step(&mut app, 2);

    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Strut);
    app.update();
    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(130.0, 0.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(
        joint_count(&mut app),
        1,
        "edge landing creates the constraint"
    );
}

// ---------- Looseness + collision exclusion (revision R4/R5) ----------

#[test]
fn welded_rod_between_boxes_stays_tight_under_gravity() {
    let mut app = crate::harness::headless_app();
    let mut left = box_record(Vec2::new(-120.0, 0.0), 40.0, 40.0);
    left.physics.rigid_body = avian2d::prelude::RigidBody::Static;
    let right = box_record(Vec2::new(120.0, 0.0), 40.0, 40.0);
    let (left_id, right_id) = (left.id, right.id);
    app.world_mut()
        .write_message(SpawnBodyIntent { record: left });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: right });
    app.update();
    // Weld-ended rod bridging the boxes; the dynamic right box hangs off it.
    let mut spec = rod_spec(
        Vec2::new(-100.0, 0.0),
        Vec2::new(100.0, 0.0),
        Some(left_id),
        Some(right_id),
    );
    for joint in &mut spec.joints {
        joint.def.kind = JointKind::Weld;
    }
    // Anchor each end joint at the rod end's location in the box's local
    // frame (rod_spec defaults to the box center, 20 px off).
    spec.joints[0].def.anchor_b = Vec2::new(20.0, 0.0);
    spec.joints[1].def.anchor_b = Vec2::new(-20.0, 0.0);
    let rod_id = spec.bodies[0].id;
    app.world_mut().write_message(SpawnRodIntent { spec });
    crate::harness::step(&mut app, 240);

    // The rod's left end must stay pinned at its authored anchor: the
    // "looseness" regression guard.
    let rod = entity_of(&app, rod_id).unwrap();
    let pose = PosRot::from_transform(app.world().get::<Transform>(rod).unwrap());
    let end_a = pose.pos + Vec2::from_angle(pose.rot).rotate(Vec2::new(-100.0, 0.0));
    let sag = (end_a - Vec2::new(-100.0, 0.0)).length();
    assert!(sag < 6.0, "welded rod end must stay tight (drift {sag} px)");
}

#[test]
fn flexure_pair_does_not_collide_when_collide_connected_is_off() {
    let mut app = weightless_app();
    // Two boxes overlapping by 10 px: without pair exclusion, contacts
    // shove them apart.
    let a = box_record(Vec2::new(0.0, 0.0), 40.0, 40.0);
    let b = box_record(Vec2::new(30.0, 0.0), 40.0, 40.0);
    let (id_a, id_b) = (a.id, b.id);
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();
    // Flexure at rest (length == chord): the beam itself exerts nothing.
    app.world_mut().write_message(SpawnRodIntent {
        spec: flexure_spec(id_a, Vec2::ZERO, id_b, Vec2::new(30.0, 0.0), 30.0),
    });
    // Let the joint derive (the frame before it exists applies one
    // contact kick in this artificial setup; the tool flow derives the
    // collider and joint in the same sync pass), then discard the kick
    // and verify no further contact impulses ever fire.
    crate::harness::step(&mut app, 3);
    let mut q = app.world_mut().query_filtered::<(
        Option<&avian2d::prelude::DistanceJoint>,
        Option<&avian2d::prelude::JointCollisionDisabled>,
    ), With<Joint>>();
    let (ghost, disabled) = q.single(app.world()).unwrap();
    assert!(ghost.is_some(), "ghost joint derived");
    assert!(disabled.is_some(), "collision-disabled marker present");
    reset_velocities(&mut app, &[id_a, id_b]);
    crate::harness::step(&mut app, 30);

    let va = velocity_of(&mut app, id_a);
    let vb = velocity_of(&mut app, id_b);
    assert!(
        va.length() < 0.5 && vb.length() < 0.5,
        "no contact shove between the connected pair (va {va}, vb {vb})"
    );
}

#[test]
fn welded_overlapping_pair_does_not_collide_probe() {
    let mut app = weightless_app();
    let a = box_record(Vec2::new(0.0, 0.0), 40.0, 40.0);
    let b = box_record(Vec2::new(30.0, 0.0), 40.0, 40.0);
    let (id_a, id_b) = (a.id, b.id);
    app.world_mut().write_message(SpawnBodyIntent { record: a });
    app.world_mut().write_message(SpawnBodyIntent { record: b });
    app.update();
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: id_a,
                body_b: Some(id_b),
                anchor_a: Vec2::new(15.0, 0.0),
                anchor_b: Vec2::new(-15.0, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    crate::harness::step(&mut app, 3);
    reset_velocities(&mut app, &[id_a, id_b]);
    crate::harness::step(&mut app, 30);
    let va = velocity_of(&mut app, id_a);
    let vb = velocity_of(&mut app, id_b);
    assert!(
        va.length() < 0.5 && vb.length() < 0.5,
        "hinged overlapping pair must not collide (va {va}, vb {vb})"
    );
}

// ---------- Rigid ↔ flexure conversion (revision R3) ----------

#[test]
fn rod_flexure_toggle_round_trips_and_undoes() {
    use gradiance::command::intent::SetRodFlexureIntent;
    let mut app = paused_app();
    let base = box_record(Vec2::ZERO, 40.0, 40.0);
    let base_id = base.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: base });
    app.update();
    // Rigid rod: attached to the box at end A, free at end B.
    let spec = rod_spec(
        Vec2::new(0.0, 0.0),
        Vec2::new(120.0, 0.0),
        Some(base_id),
        None,
    );
    let rod_id = spec.bodies[0].id;
    app.world_mut().write_message(SpawnRodIntent { spec });
    app.update();
    assert_eq!(body_count(&mut app), 2);

    // Enable flexure: capsule out; elastica joint + one tip body in.
    app.world_mut().write_message(SetRodFlexureIntent {
        target: rod_id,
        enable: true,
    });
    app.update();
    assert!(entity_of(&app, rod_id).is_none(), "capsule replaced");
    assert_eq!(body_count(&mut app), 2, "box + tip body");
    let elastica_id = {
        let mut q = app.world_mut().query::<(&StableId, &JointDef)>();
        let (id, def) = q
            .iter(app.world())
            .find(|(_, def)| matches!(def.kind, JointKind::Elastica { .. }))
            .expect("elastica joint authored");
        assert!(def.body_b.is_some());
        *id
    };
    let tip_count = app
        .world_mut()
        .query::<&gradiance::domain::rod::RodTip>()
        .iter(app.world())
        .count();
    assert_eq!(tip_count, 1, "free end got a tip body");

    // Undo restores the rigid rod exactly.
    undo(&mut app);
    assert!(entity_of(&app, rod_id).is_some(), "undo restores the rod");
    assert_eq!(joint_count(&mut app), 1, "end joint restored");

    // Redo, then convert back to rigid via the elastica joint.
    redo(&mut app);
    app.world_mut().write_message(SetRodFlexureIntent {
        target: elastica_id,
        enable: false,
    });
    app.update();
    let mut q = app.world_mut().query::<(&Rod, &ShapeDef)>();
    let (rod, shape) = q.iter(app.world()).next().expect("rigid rod again");
    assert!(matches!(shape, ShapeDef::Capsule { .. }));
    assert_eq!(rod.end_a, RodEndKind::Hinge);
    let tips = app
        .world_mut()
        .query::<&gradiance::domain::rod::RodTip>()
        .iter(app.world())
        .count();
    assert_eq!(tips, 0, "tip body absorbed");
    assert_eq!(
        joint_count(&mut app),
        1,
        "one end joint for the real attachment"
    );
}

// ---------- Pause-mode kinematic follow (revision R6) ----------

#[test]
fn moving_an_attached_body_carries_its_rod_and_undoes_together() {
    use gradiance::core::states::ToolState;
    let mut app = paused_app();
    let base = box_record(Vec2::ZERO, 40.0, 40.0);
    let base_id = base.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: base });
    app.update();
    let spec = rod_spec(
        Vec2::new(0.0, 0.0),
        Vec2::new(100.0, 0.0),
        Some(base_id),
        None,
    );
    let rod_id = spec.bodies[0].id;
    app.world_mut().write_message(SpawnRodIntent { spec });
    crate::harness::step(&mut app, 2);

    // Select-move the box by 60 px in pause mode.
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Select);
    app.update();
    // Press well away from the joint anchor at the origin (a press within
    // the glyph pick radius selects the joint instead).
    set_cursor(&mut app, Vec2::new(-14.0, 12.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(-14.0, 72.0));
    app.update();
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    let base_entity = entity_of(&app, base_id).unwrap();
    let base_pose = PosRot::from_transform(app.world().get::<Transform>(base_entity).unwrap());
    assert!(
        (base_pose.pos.y - 60.0).abs() < 5.0,
        "box moved (box pos = {})",
        base_pose.pos
    );
    let rod = entity_of(&app, rod_id).unwrap();
    let rod_pose = PosRot::from_transform(app.world().get::<Transform>(rod).unwrap());
    assert!(
        (rod_pose.pos.y - 60.0).abs() < 5.0,
        "rod followed the moved box (rod y = {})",
        rod_pose.pos.y
    );

    undo(&mut app);
    let rod = entity_of(&app, rod_id).unwrap();
    let rod_pose = PosRot::from_transform(app.world().get::<Transform>(rod).unwrap());
    assert!(
        rod_pose.pos.y.abs() < 1.0,
        "undo restores the rod too (rod y = {})",
        rod_pose.pos.y
    );
}
