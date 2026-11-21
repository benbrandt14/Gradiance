//! Air drag and wind (brief N-4): global linear and quadratic drag and a
//! wind vector in `SimSettings`. Quadratic drag uses the body's projected
//! width across the relative air velocity, so a plate falls slower face-on
//! than edge-on. The settings are scene content (saved, one undo step per
//! settled edit); the force is derived.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, paused_app, redo, step, undo};
use avian2d::prelude::{AngularVelocity, ComputedMass, LinearVelocity};
use bevy::prelude::*;
use gradiance::domain::settings::SimSettings;
use gradiance::op::invoke;
use gradiance::physics::air::AirPanels;
use gradiance::prelude::*;
use gradiance::units::{Acceleration2, Density, Velocity2};
use serde_json::json;

/// Sea-level air.
const RHO: f32 = 1.2;

fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn set_air(app: &mut App, edit: impl FnOnce(&mut SimSettings)) {
    edit(&mut app.world_mut().resource_mut::<SimSettings>());
    app.update();
}

fn velocity(app: &App, id: StableId) -> Vec2 {
    app.world()
        .get::<LinearVelocity>(entity_of(app, id).unwrap())
        .map_or(Vec2::ZERO, |v| v.0)
}

fn mass(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<ComputedMass>(entity_of(app, id).unwrap())
        .unwrap()
        .value()
}

fn gravity(app: &App) -> f32 {
    app.world()
        .resource::<SimSettings>()
        .gravity
        .value()
        .length()
}

/// The brief's behaviour test: a box falling through still air settles at
/// the terminal speed `sqrt(2 m g / (rho Cd A))`, `A` its width across the
/// fall (the world is 1 m deep), within 2 %.
#[test]
fn a_falling_box_reaches_the_quadratic_terminal_velocity() {
    let mut app = headless_app();
    set_air(&mut app, |s| {
        s.air_density = Density(RHO);
        s.drag_coefficient = 1.0;
    });
    // 2 m wide, 0.5 m tall: face-on to the fall.
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 2.0, 0.5));
    step(&mut app, 360);

    let (m, g, width, cd) = (mass(&app, body), gravity(&app), 2.0, 1.0);
    let terminal = (2.0 * m * g / (RHO * cd * width)).sqrt();
    let v = velocity(&app, body);
    assert!(
        (-v.y - terminal).abs() < 0.02 * terminal,
        "fall speed {} vs terminal {terminal}",
        -v.y
    );
    assert!(v.x.abs() < 1e-3, "drag does not push sideways ({v})");
    // Steady: another second changes nothing.
    step(&mut app, 60);
    let later = velocity(&app, body);
    assert!(
        (later.y - v.y).abs() < 0.005 * terminal,
        "{later} vs {v}: the speed is terminal"
    );
}

/// The width is the one across the flow: the same plate falls slower
/// face-on than edge-on (here turned a quarter), as `sqrt(1 / w)`.
#[test]
fn a_plate_falls_slower_face_on_than_edge_on() {
    let mut app = headless_app();
    set_air(&mut app, |s| s.drag_coefficient = 1.0);
    let face = spawn_body(&mut app, box_record(Vec2::new(-5.0, 0.0), 2.0, 0.2));
    let mut edge = box_record(Vec2::new(5.0, 0.0), 2.0, 0.2);
    edge.pose.rot = core::f32::consts::FRAC_PI_2;
    let edge = spawn_body(&mut app, edge);
    step(&mut app, 600);

    let (face_v, edge_v) = (-velocity(&app, face).y, -velocity(&app, edge).y);
    assert!(face_v > 0.0 && edge_v > face_v, "{face_v} vs {edge_v}");
    // Same mass, widths 2 m and 0.2 m: terminal speeds differ by sqrt(10).
    let ratio = edge_v / face_v;
    assert!(
        (ratio - 10_f32.sqrt()).abs() < 0.05 * 10_f32.sqrt(),
        "{ratio}"
    );
}

/// Linear drag alone: terminal speed `m g / (c w)`.
#[test]
fn linear_drag_settles_at_mg_over_cw() {
    let mut app = headless_app();
    set_air(&mut app, |s| s.linear_drag = 2.0);
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
    step(&mut app, 240);
    let terminal = mass(&app, body) * gravity(&app) / (2.0 * 1.0);
    let v = -velocity(&app, body).y;
    assert!((v - terminal).abs() < 0.02 * terminal, "{v} vs {terminal}");
}

/// Wind: with gravity off a body at rest is carried until it moves with
/// the air; with drag off the wind does nothing.
#[test]
fn the_wind_carries_a_body_through_drag_only() {
    let mut app = headless_app();
    set_air(&mut app, |s| {
        s.gravity = Acceleration2(Vec2::ZERO);
        s.wind = Velocity2(Vec2::new(3.0, -1.0));
    });
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
    step(&mut app, 60);
    assert!(
        velocity(&app, body).length() < 1e-4,
        "no drag, no push ({})",
        velocity(&app, body)
    );

    set_air(&mut app, |s| s.linear_drag = 2.0);
    // Time constant m / (c w) = 0.5 s: after 4 s the body moves with the air.
    step(&mut app, 240);
    let v = velocity(&app, body);
    assert!((v - Vec2::new(3.0, -1.0)).length() < 0.01, "{v}");
}

/// The default scene has no drag: a body falls at g, as before N-4.
#[test]
fn drag_is_off_by_default() {
    let mut app = headless_app();
    let defaults = SimSettings::default();
    assert_eq!(defaults.drag_coefficient, 0.0);
    assert_eq!(defaults.linear_drag, 0.0);
    assert_eq!(defaults.wind, Velocity2(Vec2::ZERO));
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 2.0, 0.5));
    step(&mut app, 2);
    let v0 = velocity(&app, body).y;
    step(&mut app, 60);
    let accel = (velocity(&app, body).y - v0) / 1.0;
    assert!(
        (accel + gravity(&app)).abs() < 0.01 * gravity(&app),
        "free fall at g ({accel})"
    );
}

/// The profile is derived from the shape and follows a shape edit; it is
/// never part of the saved record.
#[test]
fn the_profile_is_derived_and_follows_the_shape() {
    let mut app = headless_app();
    let body = spawn_body(&mut app, box_record(Vec2::ZERO, 2.0, 0.5));
    let profile = |app: &App| {
        app.world()
            .get::<AirPanels>(entity_of(app, body).unwrap())
            .unwrap()
            .0
    };
    assert!((profile(&app).facing_width(Vec2::Y) - 2.0).abs() < 1e-4);
    let entity = entity_of(&app, body).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .insert(ShapeDef::Circle { radius: 0.25 });
    app.update();
    assert!((profile(&app).facing_width(Vec2::Y) - 0.5).abs() < 0.01);
    let saved =
        gradiance::scene::to_ron(&gradiance::scene::SceneRecord::capture(app.world_mut())).unwrap();
    assert!(!saved.contains("AirPanels") && !saved.contains("panels"));
    assert!(saved.contains("drag_coefficient"), "the settings are saved");
}

/// A drag edit through `sim.set` is scene content: one undo step.
#[test]
fn a_drag_edit_is_one_undo_step() {
    let mut app = paused_app();
    spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
    step(&mut app, 3);
    invoke(
        app.world_mut(),
        "sim.set",
        json!({"path": "drag_coefficient", "value": 0.8}),
    )
    .unwrap();
    invoke(
        app.world_mut(),
        "sim.set",
        json!({"path": "wind.x", "value": 4.0}),
    )
    .unwrap();
    step(&mut app, 3);
    let sim = app.world().resource::<SimSettings>().clone();
    assert!((sim.drag_coefficient - 0.8).abs() < 1e-6);
    assert!((sim.wind.value().x - 4.0).abs() < 1e-6);
    undo(&mut app);
    let sim = app.world().resource::<SimSettings>().clone();
    assert_eq!(sim.drag_coefficient, 0.0, "undo reverts the drag");
    assert_eq!(sim.wind.value().x, 0.0, "and the wind, in the same step");
    redo(&mut app);
    assert!((app.world().resource::<SimSettings>().drag_coefficient - 0.8).abs() < 1e-6);
}

fn spin(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<AngularVelocity>(entity_of(app, id).unwrap())
        .map_or(0.0, |w| w.0)
}

fn rotation(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<Transform>(entity_of(app, id).unwrap())
        .unwrap()
        .rotation
        .to_euler(EulerRot::ZYX)
        .0
}

/// A dart (brief Q-6): a 2 m shaft with a disc at its +x end, dropped
/// level, turns into the flow like a weathervane and falls disc first.
#[test]
fn a_dart_turns_nose_first() {
    let mut app = headless_app();
    set_air(&mut app, |s| {
        s.air_density = Density(RHO);
        s.drag_coefficient = 1.0;
    });
    let mut dart = box_record(Vec2::new(0.0, 0.0), 2.0, 0.1);
    dart.shape = ShapeDef::Csg {
        op: gradiance::domain::shape::CsgOp::Union,
        lhs: Box::new(ShapeDef::Box {
            width: 2.0,
            height: 0.1,
        }),
        rhs: Box::new(ShapeDef::Placed {
            pos: Vec2::new(1.0, 0.0),
            rot: 0.0,
            shape: Box::new(ShapeDef::Circle { radius: 0.25 }),
        }),
    };
    let body = spawn_body(&mut app, dart);
    step(&mut app, 900);
    let nose = Vec2::from_angle(rotation(&app, body));
    let off = nose.angle_to(Vec2::NEG_Y).to_degrees().abs();
    assert!(
        off < 10.0,
        "the nose points {off} degrees off straight down"
    );
}

/// A spinning disc in still air slows (angular drag), monotonically, and
/// faster in denser air; with drag off (the default) it keeps its spin.
#[test]
fn a_spinning_disc_slows_in_still_air() {
    let run = |density: f32| -> Vec<f32> {
        let mut app = headless_app();
        set_air(&mut app, |s| {
            s.gravity = Acceleration2(Vec2::ZERO);
            s.air_density = Density(density);
            s.drag_coefficient = 1.0;
        });
        let mut disc = box_record(Vec2::ZERO, 1.0, 1.0);
        disc.shape = ShapeDef::Circle { radius: 0.5 };
        let body = spawn_body(&mut app, disc);
        let entity = entity_of(&app, body).unwrap();
        app.world_mut()
            .entity_mut(entity)
            .insert(AngularVelocity(20.0));
        (0..6)
            .map(|_| {
                step(&mut app, 30);
                spin(&app, body)
            })
            .collect()
    };
    let thin = run(RHO);
    let thick = run(10.0 * RHO);
    for spins in [&thin, &thick] {
        assert!(spins[0] < 20.0, "the spin decays: {spins:?}");
        for w in spins.windows(2) {
            assert!(
                w[1] < w[0] && w[1] > 0.0,
                "monotone, never reversed: {spins:?}"
            );
        }
    }
    assert!(
        thick[5] < thin[5],
        "denser air slows it faster: {thin:?} vs {thick:?}"
    );
}

/// With both coefficients 0 (the default) drag exerts no torque either: a
/// spinning disc keeps its spin.
#[test]
fn drag_still_off_by_default() {
    let mut app = headless_app();
    set_air(&mut app, |s| s.gravity = Acceleration2(Vec2::ZERO));
    let mut disc = box_record(Vec2::ZERO, 1.0, 1.0);
    disc.shape = ShapeDef::Circle { radius: 0.5 };
    let body = spawn_body(&mut app, disc);
    let entity = entity_of(&app, body).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .insert(AngularVelocity(20.0));
    step(&mut app, 120);
    assert!(
        (spin(&app, body) - 20.0).abs() < 1e-3,
        "{}",
        spin(&app, body)
    );
}
