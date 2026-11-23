//! Persistence tests: arbitrary-scene round trips (proptest), file I/O,
//! undoable loads, and debug snapshots.

use crate::harness::{body_count, box_record, entity_of, paused_app, undo};
use avian2d::prelude::{ColliderDensity, Friction, GravityScale, Restitution, RigidBody};
use bevy::prelude::*;
use gradiance::domain::settings::{
    CameraSettings, GridSettings, GridSystem, RenderSettings, SimSettings, SnapConfig,
};
use gradiance::prelude::*;
use gradiance::scene::{from_ron, to_ron};
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
        (0.05f32..200.0, 0.01f32..10.0).prop_map(|(half_length, radius)| ShapeDef::Capsule {
            half_length,
            radius
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
                Just(CsgOp::Xor),
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
        // A fluid region (brief N-7): density and drag.
        proptest::option::of((0.0f32..10.0, 0.0f32..20.0)),
    )
        .prop_map(
            |(shape, physics, (x, y), rot, near, thickness, (r, g, b, a), groups, fluid)| {
                BodyRecord {
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
                    depth: DepthBand::metres(near, near + thickness),
                    layers: None,
                    groups,
                    field: None,
                    tracer: None,
                    fluid: fluid.map(|(density, drag)| gradiance::domain::fluid::Fluid {
                        density: gradiance::units::Density(density),
                        drag,
                    }),
                    // A starting velocity follows from the pose seeds, so the
                    // round trip covers the field without another strategy.
                    motion: (x > 0.0).then_some(gradiance::domain::motion::InitialMotion {
                        velocity: gradiance::units::Velocity2(bevy::math::Vec2::new(x, y)),
                        spin: gradiance::units::AngularVelocity(rot),
                    }),
                    optics: (y > 0.0).then_some(gradiance::domain::optics::Optics {
                        index: 1.0 + x.abs(),
                        dispersion: y.abs() * 0.001,
                    }),
                    lifetime: (x < 0.0).then_some(gradiance::domain::lifetime::Lifetime {
                        remove_on_contact: y > 0.0,
                        ttl: Some(gradiance::units::Time(1.0 + rot.abs())),
                    }),
                    surface: (y < 0.0).then_some(gradiance::domain::surface::SurfaceSpeed(
                        gradiance::units::Velocity(x + 0.5),
                    )),
                    texture: (rot > 0.0).then(|| gradiance::domain::texture::BodyTexture {
                        offset: gradiance::units::Displacement(bevy::math::Vec2::new(y, x)),
                        angle: gradiance::units::Angle(rot),
                        mode: if x > 0.0 {
                            gradiance::domain::texture::TextureMode::Raw
                        } else {
                            gradiance::domain::texture::TextureMode::Toon
                        },
                        ..gradiance::domain::texture::BodyTexture::new(
                            "images/a.png",
                            1.0 + y.abs(),
                        )
                    }),
                }
            },
        )
}

/// Raw motor scalars: `(target velocity, max effort, damping, oscillate,
/// enabled)`. The joint *kind* decides whether these become an angular
/// (rad/s, N·m) or a linear (m/s, N) motor — the two are distinct types now,
/// so the seed stays unit-agnostic and is converted at construction.
type MotorSeed = (f32, f32, f32, bool, bool);

fn motor_strategy() -> impl Strategy<Value = MotorSeed> {
    (
        -10.0f32..10.0,
        1.0f32..1.0e8,
        0.0f32..2.0,
        any::<bool>(),
        any::<bool>(),
    )
}

fn seed_angular_motor((v, effort, damping, oscillate, enabled): MotorSeed) -> AngularMotorDef {
    AngularMotorDef {
        target_velocity: gradiance::units::AngularVelocity(v),
        max_torque: gradiance::units::Torque(effort),
        damping,
        oscillate,
        enabled,
    }
}

fn seed_linear_motor((v, effort, damping, oscillate, enabled): MotorSeed) -> LinearMotorDef {
    LinearMotorDef {
        target_velocity: gradiance::units::Velocity(v),
        max_force: gradiance::units::Force(effort),
        damping,
        oscillate,
        enabled,
    }
}

/// Joint blueprint by body index (resolved against the generated bodies).
type JointSeed = (
    usize,
    Option<usize>,
    u8,
    Option<[f32; 2]>,
    Option<MotorSeed>,
);

fn joint_seed_strategy() -> impl Strategy<Value = JointSeed> {
    (
        0usize..16,
        proptest::option::of(0usize..16),
        0u8..3,
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
        (any::<bool>(), 0.0f32..0.5, 0.0f32..1.0),
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
                        0 => JointKind::Hinge {
                            limits: limits.map(|l| l.map(gradiance::units::Angle)),
                            motor: motor.map(seed_angular_motor),
                        },
                        1 => JointKind::Slider {
                            axis: Vec2::Y,
                            limits: limits.map(|l| l.map(gradiance::units::Length)),
                            motor: motor.map(seed_linear_motor),
                        },
                        _ => JointKind::Slider {
                            axis: Vec2::X,
                            limits: limits.map(|l| l.map(gradiance::units::Length)),
                            motor: motor.map(seed_linear_motor),
                        },
                    };
                    Some(JointRecord {
                        id: StableId::new(),
                        def: JointDef {
                            kind,
                            common: JointCommon::default(),
                            body_a,
                            body_b,
                            anchor_a: gradiance::units::Displacement(Vec2::new(5.0, -3.0)),
                            anchor_b: gradiance::units::Displacement(Vec2::new(-2.0, 4.0)),
                            rest_rot_a: gradiance::units::Angle(0.0),
                            rest_rot_b: gradiance::units::Angle(0.0),
                        },
                    })
                })
                .collect();
            let mut scene = SceneRecord {
                version: gradiance::scene::FORMAT_VERSION,
                app_version: String::new(),
                bodies,
                joints,
                nodes: vec![],
                environment: EnvironmentRecord {
                    sim: SimSettings {
                        gravity: gradiance::units::Acceleration2(Vec2::new(gravity.0, gravity.1)),
                        speed,
                        // Air drag and wind (brief N-4) ride along.
                        drag_coefficient: speed,
                        linear_drag: gravity.0.abs(),
                        wind: gradiance::units::Velocity2(Vec2::new(gravity.1, speed)),
                        ..SimSettings::default()
                    },
                    grid: GridSettings {
                        system: GridSystem::Isometric,
                        ..GridSettings::default()
                    },
                    snap: SnapConfig::default(),
                    camera: CameraSettings {
                        zoom_step: 0.8,
                        ..CameraSettings::default()
                    },
                    render: RenderSettings {
                        toon_enabled: render.0,
                        dither: render.1,
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
        version: gradiance::scene::FORMAT_VERSION,
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
                    limits: Some([gradiance::units::Angle(-1.0), gradiance::units::Angle(1.0)]),
                    motor: Some(AngularMotorDef::default()),
                },
                common: JointCommon::default(),
                body_a: a,
                body_b: Some(b),
                anchor_a: gradiance::units::Displacement(Vec2::new(15.0, 0.0)),
                anchor_b: gradiance::units::Displacement(Vec2::new(-15.0, 0.0)),
                rest_rot_a: gradiance::units::Angle(0.0),
                rest_rot_b: gradiance::units::Angle(0.0),
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
        version: gradiance::scene::FORMAT_VERSION,
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
fn pre_si_files_are_rejected() {
    // v6 flipped the world to SI (metres/kg/N); older pixel-scale files are
    // not migrated (a units-only v5→v6 migration is a planned follow-up —
    // `docs/units-decision.md`). A v5 file is rejected with a version error
    // rather than silently loaded at the wrong scale.
    let scene = SceneRecord {
        version: 5,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 0.1, 0.1)],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    assert!(
        matches!(
            from_ron(&text),
            Err(gradiance::scene::PersistError::Version(5))
        ),
        "pre-SI (v5) files are rejected, not silently loaded"
    );
}

#[test]
fn v6_files_are_rejected() {
    // v7 saves every typed quantity bare (`near: 0.5`); v6 wrote the joint
    // motor fields and `break_force` wrapped (`target_velocity: (2.0)`). The
    // format is pre-release, so v6 is rejected rather than migrated
    // (`docs/units-decision.md` ruling 2).
    let scene = SceneRecord {
        version: 6,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 0.1, 0.1)],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    assert!(
        matches!(
            from_ron(&text),
            Err(gradiance::scene::PersistError::Version(6))
        ),
        "v6 files are rejected, not silently loaded"
    );
}

#[test]
fn v7_files_are_rejected() {
    // v8 authors a spring as a frequency and a damping ratio (brief 4b-4);
    // a v7 spring's stiffness and damping have no v8 meaning without the
    // bodies' mass, so v7 is rejected rather than migrated (owner ruling:
    // the format is pre-release).
    let scene = SceneRecord {
        version: 7,
        app_version: String::new(),
        bodies: vec![box_record(Vec2::ZERO, 0.1, 0.1)],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    assert!(
        matches!(
            from_ron(&text),
            Err(gradiance::scene::PersistError::Version(7))
        ),
        "v7 files are rejected, not silently loaded"
    );
}

#[test]
fn pre_tracer_files_still_parse() {
    // `tracer` is serde-defaulted, so scenes saved before tracers existed
    // load unchanged.
    let scene = SceneRecord {
        version: gradiance::scene::FORMAT_VERSION,
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
fn pre_alignment_snap_configs_still_parse() {
    // `align_edges` / `align_centers` (brief 1-5) are serde-defaulted, so
    // scenes saved before alignment snapping load with its defaults.
    let scene = SceneRecord {
        version: gradiance::scene::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord::default(),
    };
    let text = to_ron(&scene).unwrap();
    let cut = text
        .replace("align_edges: true,", "")
        .replace("align_centers: false,", "");
    assert!(!cut.contains("align_"), "fixture removed both fields");
    let parsed = from_ron(&cut).expect("pre-alignment file parses");
    assert_eq!(parsed.environment.snap, SnapConfig::default());
    assert!(parsed.environment.snap.align_edges);
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
        version: gradiance::scene::FORMAT_VERSION,
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
fn camera_settings_round_trip_and_pre_camera_files_parse() {
    let camera = CameraSettings {
        zoom_step: 0.8,
        pan_speed: 250.0,
        orbit_speed_deg: 0.75,
        arrow_scale: 0.5,
    };
    let scene = SceneRecord {
        version: gradiance::scene::FORMAT_VERSION,
        app_version: String::new(),
        bodies: vec![],
        joints: vec![],
        nodes: vec![],
        environment: EnvironmentRecord {
            camera: camera.clone(),
            ..EnvironmentRecord::default()
        },
    };
    let text = to_ron(&scene).unwrap();
    // Workstation config still travels with the file.
    let mut world = bare_world();
    from_ron(&text).unwrap().apply(&mut world);
    assert_eq!(*world.resource::<CameraSettings>(), camera);
    assert_eq!(SceneRecord::capture(&mut world).environment.camera, camera);

    // A file written before the camera section existed loads with defaults.
    let start = text.find("camera:").expect("camera field serialized");
    let end = start + text[start..].find("),").expect("camera block closes") + 2;
    let legacy = format!("{}{}", &text[..start], &text[end..]);
    assert!(!legacy.contains("zoom_step"), "{legacy}");
    let parsed = from_ron(&legacy).expect("pre-camera files must load");
    assert_eq!(parsed.environment.camera, CameraSettings::default());
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
