//! Sleeping (islands on): bodies at rest park instead of being re-solved
//! forever, and every authored change that must reach a sleeping body wakes
//! it — a shape or scale edit, a teleport, a joint change, a motor, a
//! thruster, a fluid's push. The islands plugin is enabled in
//! `physics::GradiancePhysicsPlugin`; these tests are its behaviour contract
//! and the regression pin for the avian 0.7 island panic that once forced it
//! off (see `physics` crate docs).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, step};
use avian2d::prelude::{AngularVelocity, Sleeping, SleepingDisabled};
use bevy::prelude::*;
use gradiance::command::intent::SpawnNodeIntent;
use gradiance::domain::joint::{AngularMotorDef, JointCommon, JointDef, JointKind};
use gradiance::domain::node::{NodeAttachment, NodeKind};
use gradiance::domain::thruster::Thruster;
use gradiance::prelude::*;
use gradiance::scene::{JointRecord, NodeRecord};
use gradiance::units::{Angle, AngularVelocity as Omega, Displacement, Force};

/// A static half-plane ground, its surface at `y = 0`.
fn ground(app: &mut App) -> StableId {
    let mut record = box_record(Vec2::ZERO, 1.0, 1.0);
    record.shape = ShapeDef::HalfPlane;
    record.physics = BodyPhysics::fixed();
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// A dynamic box `w` x `h` resting on the ground (bottom at `y = 0`).
fn resting_box(app: &mut App, x: f32, y: f32, w: f32, h: f32) -> StableId {
    let record = box_record(Vec2::new(x, y + h / 2.0), w, h);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// Spawns one fully-specified authored record.
fn spawn_joint(app: &mut App, record: JointRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    id
}

fn spawn_node(app: &mut App, record: NodeRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    id
}

fn is_asleep(app: &App, id: StableId) -> bool {
    app.world()
        .get::<Sleeping>(entity_of(app, id).unwrap())
        .is_some()
}

fn is_sleep_guarded(app: &App, id: StableId) -> bool {
    app.world()
        .get::<SleepingDisabled>(entity_of(app, id).unwrap())
        .is_some()
}

fn pose(app: &App, id: StableId) -> Vec2 {
    app.world()
        .get::<Transform>(entity_of(app, id).unwrap())
        .unwrap()
        .translation
        .truncate()
}

/// Steps until the body sleeps, up to `limit` frames; returns whether it did.
fn sleeps_within(app: &mut App, id: StableId, limit: usize) -> bool {
    for _ in 0..limit {
        if is_asleep(app, id) {
            return true;
        }
        step(app, 1);
    }
    is_asleep(app, id)
}

/// A box resting on the ground parks instead of being re-solved forever.
#[test]
fn a_resting_box_sleeps() {
    let mut app = headless_app();
    ground(&mut app);
    let box_id = resting_box(&mut app, 0.0, 0.001, 0.4, 0.4);
    // Well past the default 0.5 s below-threshold window.
    assert!(
        sleeps_within(&mut app, box_id, 240),
        "a box at rest on the ground should sleep"
    );
}

/// A scale edit regenerates the collider and must wake the sleeping body so
/// it re-settles against its neighbours (the `sync_colliders` wake seam).
#[test]
fn a_shape_edit_wakes_a_sleeping_box() {
    let mut app = headless_app();
    ground(&mut app);
    let box_id = resting_box(&mut app, 0.0, 0.001, 0.4, 0.4);
    assert!(sleeps_within(&mut app, box_id, 240));
    let pivot = pose(&app, box_id);
    app.world_mut().write_message(ScaleIntent {
        targets: vec![box_id],
        pivot,
        frame_rot: 0.0,
        factors: Vec2::new(2.0, 1.0),
    });
    step(&mut app, 2);
    assert!(!is_asleep(&app, box_id), "the edit must have woken it");
    assert!(
        sleeps_within(&mut app, box_id, 240),
        "and it may sleep again once it has re-settled"
    );
}

/// A committed teleport (move/rotate) writes the pose directly; avian's own
/// change-tick wake path must carry it to a sleeping body.
#[test]
fn a_teleport_wakes_a_sleeping_box() {
    let mut app = headless_app();
    ground(&mut app);
    let box_id = resting_box(&mut app, 0.0, 0.001, 0.4, 0.4);
    assert!(sleeps_within(&mut app, box_id, 240));
    let old_pos = pose(&app, box_id);
    app.world_mut().write_message(CommitTransformIntent {
        changes: vec![TransformChange {
            id: box_id,
            old: PosRot {
                pos: old_pos,
                rot: 0.0,
            },
            new: PosRot {
                pos: Vec2::new(2.0, 0.21),
                rot: 0.25,
            },
        }],
    });
    step(&mut app, 2);
    assert!(!is_asleep(&app, box_id), "the teleport must have woken it");
}

/// The old island-panic repro (avian 0.7): a joint authored between two
/// bodies that are already touching inserts `JointCollisionDisabled` and
/// removes their contact pair mid-step. With islands on, this must neither
/// panic nor explode; the pair settles and sleeps as one island.
#[test]
fn a_joint_between_touching_boxes_never_panics() {
    let mut app = headless_app();
    ground(&mut app);
    let lower = resting_box(&mut app, 0.0, 0.001, 0.6, 0.4);
    let upper = resting_box(&mut app, 0.0, 0.401, 0.6, 0.4);
    // `JointCommon::default()` has `collide_connected: false` — exactly the
    // `JointCollisionDisabled` insert on already-touching bodies.
    let joint = spawn_joint(
        &mut app,
        JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: lower,
                body_b: Some(upper),
                anchor_a: Displacement(Vec2::new(0.0, 0.2)),
                anchor_b: Displacement(Vec2::new(0.0, -0.2)),
                rest_rot_a: Angle(0.0),
                rest_rot_b: Angle(0.0),
            },
        },
    );
    let _ = joint;
    step(&mut app, 600);
    // No panic above; and the pair is finite, together, and eventually
    // asleep as one island.
    let (p_lower, p_upper) = (pose(&app, lower), pose(&app, upper));
    assert!(p_lower.is_finite() && p_upper.is_finite());
    assert!(
        (p_lower - p_upper).length() < 1.0,
        "the hinge holds: {p_lower} vs {p_upper}"
    );
    assert!(sleeps_within(&mut app, lower, 240));
    assert!(
        is_asleep(&app, upper),
        "a jointed pair sleeps as one island"
    );
}

/// An enabled motor keeps its bodies awake (`Box2D`'s rule) so a motorized
/// wheel keeps turning; the guard is the `SleepingDisabled` marker.
#[test]
fn an_enabled_motor_keeps_its_bodies_awake() {
    let mut app = headless_app();
    ground(&mut app);
    let wheel = resting_box(&mut app, 0.0, 0.001, 0.6, 0.6);
    spawn_joint(
        &mut app,
        JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: Some(AngularMotorDef {
                        target_velocity: Omega(1.0),
                        ..AngularMotorDef::default()
                    }),
                },
                common: JointCommon::default(),
                body_a: wheel,
                body_b: None,
                anchor_a: Displacement(Vec2::ZERO),
                anchor_b: Displacement(Vec2::ZERO),
                rest_rot_a: Angle(0.0),
                rest_rot_b: Angle(0.0),
            },
        },
    );
    step(&mut app, 90);
    assert!(
        is_sleep_guarded(&app, wheel),
        "an enabled motor parks the no-sleep guard"
    );
    assert!(!is_asleep(&app, wheel), "the wheel never sleeps");
    let spin = app
        .world()
        .get::<AngularVelocity>(entity_of(&app, wheel).unwrap())
        .unwrap()
        .0;
    assert!(spin.abs() > 0.1, "the wheel keeps turning ({spin})");
}

/// A motorless joint (a plain hinge) does not keep its bodies awake: the
/// guard is lifted and the pair may sleep.
#[test]
fn a_motorless_hinge_lets_its_bodies_sleep() {
    let mut app = headless_app();
    ground(&mut app);
    let lower = resting_box(&mut app, 0.0, 0.001, 0.6, 0.4);
    let upper = resting_box(&mut app, 0.0, 0.401, 0.6, 0.4);
    spawn_joint(
        &mut app,
        JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: lower,
                body_b: Some(upper),
                anchor_a: Displacement(Vec2::new(0.0, 0.2)),
                anchor_b: Displacement(Vec2::new(0.0, -0.2)),
                rest_rot_a: Angle(0.0),
                rest_rot_b: Angle(0.0),
            },
        },
    );
    assert!(
        sleeps_within(&mut app, lower, 360),
        "a motorless joint does not keep the pair awake"
    );
    assert!(
        !is_sleep_guarded(&app, lower) && !is_sleep_guarded(&app, upper),
        "no guard is parked for a motorless joint"
    );
}

/// A thruster is a user action, not a constant of the scene: its push must
/// wake the body it acts on (`apply_thrusters` wake seam).
#[test]
fn a_thruster_wakes_a_sleeping_box() {
    let mut app = headless_app();
    ground(&mut app);
    let box_id = resting_box(&mut app, 0.0, 0.001, 0.4, 0.4);
    assert!(sleeps_within(&mut app, box_id, 240));
    let start = pose(&app, box_id);
    spawn_node(
        &mut app,
        NodeRecord {
            id: StableId::new(),
            pose: PosRot {
                pos: start,
                rot: 0.0,
            },
            attachment: NodeAttachment::to(box_id, Vec2::ZERO),
            kind: NodeKind::Thruster(Thruster {
                thrust: Force(40.0),
                angle: Angle(core::f32::consts::FRAC_PI_2),
            }),
            appearance: Appearance::default(),
        },
    );
    step(&mut app, 30);
    assert!(!is_asleep(&app, box_id), "the push must have woken it");
    assert!(
        pose(&app, box_id).y > start.y + 0.5,
        "and lifted it ({} -> {:?})",
        start.y,
        pose(&app, box_id).y
    );
}

/// A submerged body never sleeps: buoyancy keeps waking it so the bobbing
/// converges to the waterline instead of freezing off-equilibrium.
#[test]
fn a_floating_body_stays_awake() {
    let mut app = headless_app();
    // A static sensor tank of fluid of density 2, surface at y = 0.
    let mut tank = box_record(Vec2::new(0.0, -3.0), 20.0, 6.0);
    tank.physics = BodyPhysics::fixed();
    tank.physics.sensor = true;
    tank.fluid = Some(gradiance::domain::fluid::Fluid {
        density: gradiance::units::Density(2.0),
        ..gradiance::domain::fluid::Fluid::default()
    });
    let _tank_id = {
        let id = tank.id;
        app.world_mut()
            .write_message(SpawnBodyIntent { record: tank });
        app.update();
        id
    };
    // A box of half the fluid's density floats half submerged.
    let mut floater = box_record(Vec2::new(0.0, 0.6), 1.0, 1.0);
    floater.physics.density = avian2d::prelude::ColliderDensity(1.0);
    let body = {
        let id = floater.id;
        app.world_mut()
            .write_message(SpawnBodyIntent { record: floater });
        app.update();
        id
    };
    step(&mut app, 300);
    assert!(
        !is_asleep(&app, body),
        "a floating body is pushed every step and must stay awake"
    );
    // And it must have converged near its waterline rather than frozen
    // mid-bob: a density-1 box in fluid 2 floats half submerged.
    let y = pose(&app, body).y;
    assert!((y - 0.0).abs() < 0.05, "near the waterline (y = {y})");
}
