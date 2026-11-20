//! Buoyancy (brief N-7): a body marked as fluid pushes up every dynamic
//! body inside it by `ρ g A` at the submerged centroid and damps it. The
//! fluid is authored (saved, one undo step); the forces and the derived
//! region and outline are not.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, paused_app, redo, step, undo};
use avian2d::prelude::{AngularVelocity, LinearVelocity, RigidBody, Sensor};
use bevy::prelude::*;
use gradiance::domain::fluid::Fluid;
use gradiance::op::invoke;
use gradiance::physics::buoyancy::{FluidRegion, SubmergeOutline};
use gradiance::prelude::*;
use gradiance::units::Density;
use serde_json::json;

/// The fluid's density in these tests (kg/m²).
const RHO: f32 = 2.0;

fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// A static sensor tank of fluid, `w` x `h`, its surface at `y = 0`.
fn tank(app: &mut App, w: f32, h: f32, fluid: Fluid) -> StableId {
    let mut record = box_record(Vec2::new(0.0, -h / 2.0), w, h);
    record.physics = BodyPhysics::fixed();
    record.physics.sensor = true;
    record.fluid = Some(fluid);
    spawn_body(app, record)
}

fn water() -> Fluid {
    Fluid {
        density: Density(RHO),
        ..Fluid::default()
    }
}

/// A box `w` x `h` of `density` whose centre starts at `pos`.
fn floater(app: &mut App, pos: Vec2, w: f32, h: f32, density: f32) -> StableId {
    let mut record = box_record(pos, w, h);
    record.physics.density = avian2d::prelude::ColliderDensity(density);
    spawn_body(app, record)
}

fn pose(app: &App, id: StableId) -> (Vec2, f32) {
    let transform = app
        .world()
        .get::<Transform>(entity_of(app, id).unwrap())
        .unwrap();
    (
        transform.translation.truncate(),
        transform.rotation.to_euler(EulerRot::ZYX).0,
    )
}

fn speed(app: &App, id: StableId) -> (f32, f32) {
    let entity = entity_of(app, id).unwrap();
    let v = app.world().get::<LinearVelocity>(entity).unwrap().0;
    let w = app.world().get::<AngularVelocity>(entity).unwrap().0;
    (v.length(), w.abs())
}

/// The brief's behaviour test: a box of half the fluid's density floats
/// half submerged, within 3 % (of the submerged fraction).
#[test]
fn a_box_of_half_the_fluid_density_floats_half_submerged() {
    let mut app = headless_app();
    tank(&mut app, 20.0, 6.0, water());
    // A 1 m box at density 1 in fluid of density 2, dropped from just above.
    let body = floater(&mut app, Vec2::new(0.0, 0.6), 1.0, 1.0, RHO / 2.0);
    step(&mut app, 600);

    let (pos, rot) = pose(&app, body);
    // The surface is y = 0: the submerged depth is the bottom's depth.
    let fraction = (0.5 - pos.y) / 1.0;
    assert!(
        (fraction - 0.5).abs() < 0.03 * 0.5,
        "submerged fraction {fraction} (centre at {pos})"
    );
    assert!(rot.abs() < 0.01, "upright ({rot})");
    let (v, w) = speed(&app, body);
    assert!(v < 0.01 && w < 0.01, "settled ({v}, {w})");
}

/// Other density ratios float at their own waterline: a quarter-density
/// plank floats a quarter submerged in an open sea (a half-plane fluid).
#[test]
fn a_light_plank_floats_at_its_density_ratio_in_a_sea() {
    let mut app = headless_app();
    let mut sea = box_record(Vec2::ZERO, 1.0, 1.0);
    sea.shape = ShapeDef::HalfPlane;
    sea.physics = BodyPhysics::fixed();
    sea.physics.sensor = true;
    sea.fluid = Some(water());
    spawn_body(&mut app, sea);
    let body = floater(&mut app, Vec2::new(3.0, 0.5), 2.0, 0.5, RHO / 4.0);
    step(&mut app, 600);

    let (pos, _) = pose(&app, body);
    let fraction = (0.25 - pos.y) / 0.5;
    assert!(
        (fraction - 0.25).abs() < 0.03 * 0.25,
        "submerged fraction {fraction} (centre at {pos})"
    );
}

/// A body denser than the fluid sinks, slowed by the fluid's drag but never
/// held up.
#[test]
fn a_heavy_box_sinks() {
    let mut app = headless_app();
    tank(&mut app, 20.0, 40.0, water());
    let body = floater(&mut app, Vec2::new(0.0, 0.6), 1.0, 1.0, RHO * 1.5);
    step(&mut app, 180);
    let (pos, _) = pose(&app, body);
    assert!(pos.y < -2.0, "it sank ({pos})");
    let v = app
        .world()
        .get::<LinearVelocity>(entity_of(&app, body).unwrap())
        .unwrap()
        .0;
    assert!(v.y < -0.5, "still sinking ({v})");
    // Net weight (ρ_b − ρ) A g against drag ρ k A v: terminal v = (ρ_b/ρ − 1) g / k.
    let g = app
        .world()
        .resource::<gradiance::domain::settings::SimSettings>()
        .gravity
        .value()
        .length();
    let terminal = 0.5 * g / water().drag;
    assert!(
        (-v.y - terminal).abs() < 0.05 * terminal,
        "{} vs terminal {terminal}",
        -v.y
    );
}

/// A floating body dropped from high up splashes in, bobs and settles: the
/// motion is damped and never runs away. A tilted plank rights itself (the
/// push acts at the submerged centroid, which moves to the low side).
#[test]
fn a_dropped_tilted_plank_settles_upright() {
    let mut app = headless_app();
    tank(&mut app, 20.0, 6.0, water());
    let mut record = box_record(Vec2::new(0.0, 2.0), 2.0, 0.5);
    record.physics.density = avian2d::prelude::ColliderDensity(RHO / 2.0);
    record.pose.rot = 0.4;
    let body = spawn_body(&mut app, record);

    let mut peak = 0.0_f32;
    for _ in 0..20 {
        step(&mut app, 30);
        peak = peak.max(speed(&app, body).0);
    }
    // Dropped 2 m: never faster than the free fall that brought it in.
    let g = 9.81_f32;
    assert!(peak < (2.0 * g * 2.0).sqrt() * 1.1, "no runaway ({peak})");
    let (v, w) = speed(&app, body);
    assert!(v < 0.01 && w < 0.01, "settled ({v}, {w})");
    let (pos, rot) = pose(&app, body);
    assert!(rot.abs() < 0.01, "upright ({rot})");
    let fraction = (0.25 - pos.y) / 0.5;
    assert!((fraction - 0.5).abs() < 0.03 * 0.5, "{fraction}");
}

/// Making a fluid through the operation is one undo step that also makes
/// the body a static sensor; the fluid is saved, the region and outline
/// are derived and never saved, and the region follows the shape.
#[test]
fn a_fluid_is_authored_and_its_region_derived() {
    let mut app = paused_app();
    let pool = spawn_body(&mut app, box_record(Vec2::ZERO, 4.0, 2.0));
    let entity = entity_of(&app, pool).unwrap();
    assert!(app.world().get::<SubmergeOutline>(entity).is_some());
    invoke(
        app.world_mut(),
        "body.set_fluid",
        json!({"i": pool.0.to_string(), "density": 1.5, "drag": 3}),
    )
    .unwrap();
    step(&mut app, 2);
    let fluid = *app.world().get::<Fluid>(entity).unwrap();
    assert_eq!(fluid.density, Density(1.5));
    assert_eq!(fluid.drag, 3.0);
    assert!(app.world().get::<Sensor>(entity).is_some());
    assert_eq!(
        app.world().get::<RigidBody>(entity),
        Some(&RigidBody::Static)
    );
    let bounds = |app: &App| {
        app.world()
            .get::<FluidRegion>(entity_of(app, pool).unwrap())
            .unwrap()
            .0
            .bounds()
            .unwrap()
    };
    assert!((bounds(&app).1 - Vec2::new(2.0, 1.0)).length() < 1e-5);

    let saved =
        gradiance::scene::to_ron(&gradiance::scene::SceneRecord::capture(app.world_mut())).unwrap();
    assert!(saved.contains("fluid: Some"), "the fluid is saved");
    assert!(!saved.contains("FluidRegion") && !saved.contains("planes"));

    // The region follows a shape edit.
    app.world_mut().entity_mut(entity).insert(ShapeDef::Box {
        width: 6.0,
        height: 2.0,
    });
    app.update();
    assert!((bounds(&app).1 - Vec2::new(3.0, 1.0)).length() < 1e-5);

    // One undo step takes back the fluid, the sensor and the role.
    let entity = entity_of(&app, pool).unwrap();
    app.world_mut().entity_mut(entity).insert(ShapeDef::Box {
        width: 4.0,
        height: 2.0,
    });
    app.update();
    undo(&mut app);
    let entity = entity_of(&app, pool).unwrap();
    assert!(app.world().get::<Fluid>(entity).is_none());
    assert!(app.world().get::<Sensor>(entity).is_none());
    assert_eq!(
        app.world().get::<RigidBody>(entity),
        Some(&RigidBody::Dynamic)
    );
    app.update();
    assert!(app.world().get::<FluidRegion>(entity).is_none());
    redo(&mut app);
    assert!(
        app.world()
            .get::<Fluid>(entity_of(&app, pool).unwrap())
            .is_some()
    );

    // Density 0 turns the fluid off and keeps the sensor.
    invoke(
        app.world_mut(),
        "body.set_fluid",
        json!({"i": pool.0.to_string(), "density": 0}),
    )
    .unwrap();
    step(&mut app, 2);
    let entity = entity_of(&app, pool).unwrap();
    assert!(app.world().get::<Fluid>(entity).is_none());
    assert!(app.world().get::<Sensor>(entity).is_some());
}

/// Without a fluid nothing changes: a body falls at g through a sensor.
#[test]
fn a_plain_sensor_holds_no_fluid() {
    let mut app = headless_app();
    let mut record = box_record(Vec2::new(0.0, -3.0), 20.0, 6.0);
    record.physics = BodyPhysics::fixed();
    record.physics.sensor = true;
    spawn_body(&mut app, record);
    let body = floater(&mut app, Vec2::new(0.0, 0.0), 1.0, 1.0, 1.0);
    step(&mut app, 60);
    let (pos, _) = pose(&app, body);
    // About ½ g t² = 4.9 m in 1 s.
    assert!(pos.y < -4.0, "{pos}");
}
