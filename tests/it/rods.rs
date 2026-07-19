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
                common: JointCommon {
                    collide_connected: true,
                },
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
    let cache = elastica_cache(&mut app);
    let bow = cache.0.x[2].abs().max(cache.0.x[3].abs());
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

fn velocity_of(app: &mut App, id: StableId) -> Vec2 {
    let entity = entity_of(app, id).unwrap();
    app.world()
        .get::<avian2d::prelude::LinearVelocity>(entity)
        .map(|v| v.0)
        .unwrap_or_default()
}

fn elastica_cache(app: &mut App) -> gradiance::physics::elastica::ElasticaCache {
    let state = app
        .world_mut()
        .query::<&gradiance::physics::elastica::ElasticaCache>()
        .single(app.world())
        .map(|c| c.0)
        .unwrap_or_default();
    gradiance::physics::elastica::ElasticaCache(state)
}
