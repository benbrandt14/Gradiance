//! Persistence tests: arbitrary-scene round trips (proptest), file I/O,
//! undoable loads, and debug snapshots.

use crate::harness::{body_count, box_record, entity_of, paused_app, undo};
use avian2d::prelude::{ColliderDensity, Friction, GravityScale, Restitution, RigidBody};
use bevy::prelude::*;
use gradiance::domain::settings::{
    GridSettings, GridSystem, RenderSettings, SimSettings, SnapConfig,
};
use gradiance::persist::{from_ron, to_ron};
use gradiance::prelude::*;
use proptest::prelude::*;

// ---------- Arbitrary scene generation ----------

fn shape_strategy() -> impl Strategy<Value = ShapeDef> {
    prop_oneof![
        (1.0f32..400.0, 1.0f32..400.0).prop_map(|(width, height)| ShapeDef::Box { width, height }),
        (0.5f32..200.0).prop_map(|radius| ShapeDef::Circle { radius }),
        // Star-shaped n-gons: always simple, CCW, centroid-near-origin.
        (3usize..9, 10.0f32..100.0, 0.0f32..1.0).prop_map(|(n, r, jitter)| {
            let outline = (0..n)
                .map(|i| {
                    let theta = std::f32::consts::TAU * (i as f32) / (n as f32);
                    let radius = r * (1.0 + 0.3 * jitter * (i as f32).sin());
                    Vec2::new(theta.cos(), theta.sin()) * radius
                })
                .collect();
            ShapeDef::Polygon {
                outline,
                holes: vec![],
            }
        }),
        Just(ShapeDef::HalfPlane),
        // CSG trees: a leaf op a placed leaf (what cutting produces).
        (
            (1.0f32..200.0, 1.0f32..200.0),
            (0.5f32..100.0),
            (
                -100.0f32..100.0,
                -100.0f32..100.0,
                0.0f32..std::f32::consts::TAU
            ),
            prop_oneof![
                Just(CsgOp::Union),
                Just(CsgOp::Subtract),
                Just(CsgOp::Intersect),
                (0.5f32..20.0).prop_map(|radius| CsgOp::SmoothUnion { radius }),
            ],
        )
            .prop_map(|((width, height), radius, (x, y, rot), op)| ShapeDef::Csg {
                op,
                lhs: Box::new(ShapeDef::Box { width, height }),
                rhs: Box::new(ShapeDef::Placed {
                    pos: Vec2::new(x, y),
                    rot,
                    shape: Box::new(ShapeDef::Circle { radius }),
                }),
            }),
    ]
}

fn physics_strategy() -> impl Strategy<Value = BodyPhysics> {
    (
        prop_oneof![
            Just(RigidBody::Dynamic),
            Just(RigidBody::Static),
            Just(RigidBody::Kinematic)
        ],
        0.1f32..10.0,
        0.0f32..2.0,
        0.0f32..1.0,
        -2.0f32..2.0,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(
                rigid_body,
                density,
                friction,
                restitution,
                gravity_scale,
                sensor,
                rotation_locked,
            )| {
                BodyPhysics {
                    rigid_body,
                    friction: Friction::new(friction),
                    restitution: Restitution::new(restitution),
                    density: ColliderDensity(density),
                    gravity_scale: GravityScale(gravity_scale),
                    sensor,
                    rotation_locked,
                }
            },
        )
}

fn body_strategy() -> impl Strategy<Value = BodyRecord> {
    (
        shape_strategy(),
        physics_strategy(),
        (-1000.0f32..1000.0, -1000.0f32..1000.0),
        -3.0f32..3.0,
        0.0f32..300.0,
        2.5f32..100.0,
        (0.0f32..1.0, 0.0f32..1.0, 0.0f32..1.0, 0.1f32..1.0),
        proptest::collection::vec(1u32..6, 0..3),
    )
        .prop_map(
            |(shape, physics, (x, y), rot, near, thickness, (r, g, b, a), groups)| BodyRecord {
                id: StableId::new(),
                pose: PosRot {
                    pos: Vec2::new(x, y),
                    rot,
                },
                shape,
                physics,
                appearance: Appearance {
                    fill: Rgba { r, g, b, a },
                    ..Appearance::default()
                },
                depth: DepthBand {
                    near,
                    far: near + thickness,
                },
                layers: None,
                groups,
                field: None,
                tracer: None,
                rod: None,
            },
        )
}

fn motor_strategy() -> impl Strategy<Value = MotorDef> {
    (
        -10.0f32..10.0,
        1.0f32..1.0e8,
        0.0f32..2.0,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(target_velocity, max_force, damping, oscillate, enabled)| MotorDef {
                target_velocity,
                max_force,
                damping,
                oscillate,
                enabled,
            },
        )
}

/// Joint blueprint by body index (resolved against the generated bodies).
type JointSeed = (usize, Option<usize>, u8, Option<[f32; 2]>, Option<MotorDef>);

fn joint_seed_strategy() -> impl Strategy<Value = JointSeed> {
    (
        0usize..16,
        proptest::option::of(0usize..16),
        0u8..4,
        proptest::option::of((-2.0f32..0.0, 0.0f32..2.0).prop_map(|(a, b)| [a, b])),
        proptest::option::of(motor_strategy()),
    )
}

fn scene_strategy() -> impl Strategy<Value = SceneRecord> {
    (
        proptest::collection::vec(body_strategy(), 0..10),
        proptest::collection::vec(joint_seed_strategy(), 0..6),
        (-2000.0f32..2000.0, -2000.0f32..2000.0),
        0.0f32..5.0,
        (any::<bool>(), 1u32..12, 0.0f32..1.0),
    )
        .prop_map(|(bodies, seeds, gravity, speed, render)| {
            let joints = seeds
                .into_iter()
                .filter_map(|(a, b, kind, limits, motor)| {
                    let body_a = bodies.get(a)?.id;
                    let body_b = match b {
                        Some(i) => {
                            let id = bodies.get(i)?.id;
                            if id == body_a {
                                return None;
                            }
                            Some(id)
                        }
                        None => None,
                    };
                    let kind = match kind {
                        0 => JointKind::Hinge { limits, motor },
                        1 => JointKind::Slider {
                            axis: Vec2::Y,
                            limits,
                            motor,
                        },
                        2 => JointKind::Weld,
                        _ => JointKind::Slider {
                            axis: Vec2::X,
                            limits,
                            motor,
                        },
                    };
                    Some(JointRecord {
                        id: StableId::new(),
                        def: JointDef {
                            kind,
                            common: JointCommon::default(),
                            body_a,
                            body_b,
                            anchor_a: Vec2::new(5.0, -3.0),
                            anchor_b: Vec2::new(-2.0, 4.0),
                            rest_rot_a: 0.0,
                            rest_rot_b: 0.0,
                        },
                    })
                })
                .collect();
            let mut scene = SceneRecord {
                version: gradiance::persist::FORMAT_VERSION,
                app_version: String::new(),
                bodies,
                joints,
                nodes: vec![],
                environment: EnvironmentRecord {
                    sim: SimSettings {
                        gravity: Vec2::new(gravity.0, gravity.1),
                        speed,
                        ..SimSettings::default()
                    },
                    grid: GridSettings {
                        system: GridSystem::Isometric,
                        ..GridSettings::default()
                    },
                    snap: SnapConfig::default(),
                    render: RenderSettings {
                        toon_enabled: render.0,
                        bands: render.1,
                        rim_strength: render.2,
                        ..RenderSettings::default()
                    },
                    ..EnvironmentRecord::default()
                },
            };
            scene.bodies.sort_by_key(|r| r.id.0);
            scene.joints.sort_by_key(|r| r.id.0);
            scene
        })
}

/// A bare world that supports capture/apply (no app machinery — fast).
fn bare_world() -> World {
    let mut world = World::new();
    world.init_resource::<IdIndex>();
    world
}

// ---------- The round-trip property ----------

#[test]
fn any_scene_round_trips_through_world_and_ron_to_a_fixpoint() {
    let mut runner = proptest::test_runner::TestRunner::new(proptest::test_runner::Config {
        cases: 48,
        ..Default::default()
    });
    runner
        .run(&scene_strategy(), |scene| {
            // Generation 1: raw records → world → capture.
            let mut w1 = bare_world();
            scene.apply(&mut w1);
            let s1 = SceneRecord::capture(&mut w1);

            // Everything except pose must survive exactly.
            prop_assert_eq!(s1.bodies.len(), scene.bodies.len());
            prop_assert_eq!(s1.joints.len(), scene.joints.len());
            for (a, b) in scene.bodies.iter().zip(&s1.bodies) {
                prop_assert_eq!(a.id, b.id);
                prop_assert_eq!(&a.shape, &b.shape);
                prop_assert_eq!(a.physics, b.physics);
                prop_assert_eq!(a.appearance, b.appearance);
                prop_assert_eq!(a.layers, b.layers);
                prop_assert_eq!(&a.groups, &b.groups);
                prop_assert!((a.pose.pos - b.pose.pos).length() < 1e-3);
                prop_assert!((a.pose.rot - b.pose.rot).abs() < 1e-3);
            }
            for (a, b) in scene.joints.iter().zip(&s1.joints) {
                prop_assert_eq!(a.id, b.id);
                prop_assert_eq!(&a.def, &b.def);
            }
            prop_assert_eq!(&scene.environment, &s1.environment);

            // Generation 2 via RON: text → scene → world → capture.
            let text1 = to_ron(&s1).expect("serialize");
            let parsed = from_ron(&text1).expect("parse");
            let mut w2 = bare_world();
            parsed.apply(&mut w2);
            let s2 = SceneRecord::capture(&mut w2);

            // Generation 3: the fixpoint — bytes must now be identical.
            let text2 = to_ron(&s2).expect("serialize");
            let mut w3 = bare_world();
            from_ron(&text2).expect("parse").apply(&mut w3);
            let s3 = SceneRecord::capture(&mut w3);
            let text3 = to_ron(&s3).expect("serialize");
            prop_assert_eq!(text2, text3, "save→load→save reached a byte fixpoint");
            Ok(())
        })
        .unwrap();
}

// ---------- Full-app integration ----------

#[test]
fn loading_a_scene_is_one_undoable_command() {
    let mut app = paused_app();
    let keep = spawn_via_intent(&mut app, Vec2::new(7.0, 7.0));

    // A one-box scene to load.
    let incoming_body = box_record(Vec2::new(50.0, 0.0), 30.0, 30.0);
    let incoming_id = incoming_body.id;
    let scene = SceneRecord {
        version: gradiance::persist::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![incoming_body],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };

    app.world_mut().write_message(LoadSceneIntent { scene });
    app.update();
    assert_eq!(body_count(&mut app), 1, "load replaced the world");
    assert!(entity_of(&app, incoming_id).is_some());
    assert!(entity_of(&app, keep).is_none());

    undo(&mut app);
    assert_eq!(body_count(&mut app), 1, "undo restored the old world");
    assert!(entity_of(&app, keep).is_some(), "original body is back");
    assert!(entity_of(&app, incoming_id).is_none());
}

fn spawn_via_intent(app: &mut App, pos: Vec2) -> StableId {
    let record = box_record(pos, 40.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

#[test]
fn exit_autosave_writes_a_resumable_scene() {
    let dir = std::env::temp_dir().join(format!("gradiance-autosave-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.ron");

    let mut app = paused_app();
    app.insert_resource(gradiance::persist::AutosavePath(path.clone()));
    spawn_via_intent(&mut app, Vec2::new(3.0, 4.0));
    app.world_mut().write_message(AppExit::Success);
    app.update(); // Last-schedule autosave observes the exit message

    let text = std::fs::read_to_string(&path).expect("autosave written on exit");
    let scene = from_ron(&text).expect("autosave parses as a scene");
    assert_eq!(scene.bodies.len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_empty_scene_is_not_autosaved_over_the_last_session() {
    let dir = std::env::temp_dir().join(format!("gradiance-autosave-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.ron");
    std::fs::write(&path, "previous session").unwrap();

    let mut app = paused_app();
    app.insert_resource(gradiance::persist::AutosavePath(path.clone()));
    app.world_mut().write_message(AppExit::Success);
    app.update();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "previous session",
        "an empty scene must not clobber the previous autosave"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_and_load_requests_round_trip_through_a_file() {
    let dir = std::env::temp_dir().join(format!("gradiance-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scene.ron");

    let mut app = paused_app();
    let a = spawn_via_intent(&mut app, Vec2::new(12.0, -5.0));
    app.world_mut()
        .write_message(gradiance::persist::SaveSceneRequest {
            path: Some(path.clone()),
        });
    app.update();
    assert!(path.exists(), "scene file written");

    // Fresh app: load the file, world matches.
    let mut app2 = paused_app();
    app2.world_mut()
        .write_message(gradiance::persist::LoadSceneRequest {
            path: Some(path.clone()),
        });
    app2.update(); // request → intent
    app2.update(); // intent → command
    assert_eq!(body_count(&mut app2), 1);
    let entity = entity_of(&app2, a).expect("stable id survived the file");
    let pos = app2
        .world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate();
    assert!((pos - Vec2::new(12.0, -5.0)).length() < 1e-3);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn joints_survive_files_and_resolve_after_load() {
    let dir = std::env::temp_dir().join(format!("gradiance-test-j-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("jointed.ron");

    let mut app = paused_app();
    let a = spawn_via_intent(&mut app, Vec2::ZERO);
    let b = spawn_via_intent(&mut app, Vec2::new(30.0, 0.0));
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: Some([-1.0, 1.0]),
                    motor: Some(MotorDef::default()),
                },
                common: JointCommon::default(),
                body_a: a,
                body_b: Some(b),
                anchor_a: Vec2::new(15.0, 0.0),
                anchor_b: Vec2::new(-15.0, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();
    app.world_mut()
        .write_message(gradiance::persist::SaveSceneRequest {
            path: Some(path.clone()),
        });
    app.update();

    let mut app2 = paused_app();
    app2.world_mut()
        .write_message(gradiance::persist::LoadSceneRequest {
            path: Some(path.clone()),
        });
    app2.update();
    app2.update();
    app2.update(); // joint_sync resolution frame

    let mut q = app2.world_mut().query::<&JointDef>();
    let def = q.iter(app2.world()).next().expect("joint loaded").clone();
    assert_eq!(def.body_a, a);
    assert_eq!(def.body_b, Some(b));
    assert!(entity_of(&app2, a).is_some() && entity_of(&app2, b).is_some());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn pre_field_files_still_parse() {
    // `field` is serde-defaulted: files written before field sources load
    // with none (no format bump needed).
    let scene = SceneRecord {
        version: gradiance::persist::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 10.0, 10.0)],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    let cut = text.replace("field: None,", "");
    assert_ne!(text, cut, "fixture actually removed the field");
    let parsed = from_ron(&cut).expect("pre-magnet file parses");
    assert_eq!(parsed.bodies[0].field, None);
}

#[test]
fn v4_layer_masks_migrate_to_depth_bands() {
    // A v4 file authors `layers: LayerMask32`; loading maps each mask's
    // occupied bit range to the equivalent band (bits 1..=2 → 10..30) and
    // drops custom filters (collision is depth overlap in v5). The fixture
    // serializes the legacy carrier field directly, so it is immune to
    // RON formatting drift.
    let mut record = box_record(Vec2::ZERO, 10.0, 10.0);
    record.layers = Some(gradiance::domain::layers::LayerMask32 {
        memberships: 0b0110,
        filters: 0xFF,
    });
    let scene = SceneRecord {
        version: 4,
        app_version: String::new(),
        bodies: vec![record],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    let parsed = from_ron(&text).expect("v4 file migrates");
    assert_eq!(parsed.version, gradiance::persist::FORMAT_VERSION);
    assert_eq!(
        parsed.bodies[0].depth,
        DepthBand {
            near: 10.0,
            far: 30.0
        },
        "mask bits 1..=2 become the 10..30 band"
    );
    assert_eq!(parsed.bodies[0].layers, None, "carrier cleared");
}

#[test]
fn pre_tracer_files_still_parse() {
    // `tracer` is serde-defaulted, so scenes saved before tracers existed
    // load unchanged.
    let scene = SceneRecord {
        version: gradiance::persist::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 10.0, 10.0)],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    let cut = text.replace("tracer: None,", "");
    assert_ne!(text, cut, "fixture actually removed the tracer");
    let parsed = from_ron(&cut).expect("pre-tracer file parses");
    assert_eq!(parsed.bodies[0].tracer, None);
}

#[test]
fn version_mismatch_is_rejected() {
    let text = to_ron(&SceneRecord {
        version: 99,
        app_version: String::new(),
        bodies: vec![],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    })
    .unwrap();
    assert!(from_ron(&text).is_err(), "future versions must not load");
}

#[test]
fn pre_render_settings_files_still_parse() {
    // Simulate a pre-M10 save: serialize a current scene, then strip the
    // trailing `render` field from its environment block.
    let scene = SceneRecord {
        version: gradiance::persist::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 10.0, 10.0)],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    let cut_at = text.find("render:").expect("render field serialized");
    let legacy = format!("{})\n)", &text[..cut_at]);

    let parsed = from_ron(&legacy).expect("legacy files without render settings must load");
    assert_eq!(parsed.environment.render, RenderSettings::default());
    assert_eq!(parsed.bodies.len(), 1);
}

#[test]
fn snapshot_request_writes_a_timestamped_repro_file() {
    let dir = std::env::temp_dir().join(format!("gradiance-snap-{}", std::process::id()));
    let mut app = paused_app();
    spawn_via_intent(&mut app, Vec2::ZERO);
    app.world_mut()
        .write_message(gradiance::persist::SnapshotRequest {
            dir: Some(dir.clone()),
        });
    app.update();

    let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert_eq!(entries.len(), 1, "one snapshot written");
    let path = entries[0].as_ref().unwrap().path();
    let scene = from_ron(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(scene.bodies.len(), 1, "snapshot captured the live scene");
    std::fs::remove_dir_all(&dir).ok();
}
