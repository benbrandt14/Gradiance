//! Gears (brief S-5): a toothed outline, made by `spawn-gear`, the gear tool
//! or `make-gear`, that meshes with another through ordinary contact.

#![allow(clippy::unwrap_used)]

use crate::harness::{body_count, paused_app, step, undo};
use avian2d::prelude::AngularVelocity;
use bevy::prelude::*;
use gradiance::geometry::gear::SAMPLES_PER_TOOTH;
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

/// Every body, in id (spawn) order.
fn bodies(app: &mut App) -> Vec<(Entity, StableId, ShapeDef)> {
    let mut query = app.world_mut().query::<(Entity, &StableId, &ShapeDef)>();
    let mut out: Vec<_> = query
        .iter(app.world())
        .map(|(e, id, shape)| (e, *id, shape.clone()))
        .collect();
    out.sort_by_key(|(_, id, _)| id.0);
    out
}

/// The brief's behaviour test: a 12-tooth gear driven at 1 rad/s turns a
/// 24-tooth gear of the same module at 0.5 rad/s (+-5 %) over 5 s, the other
/// way round. Before the exact polygon collider this pair jammed after one
/// tooth: the approximate decomposition filled the tooth gaps (5 and 2
/// pieces).
#[test]
fn a_driven_gear_turns_its_mate_at_the_tooth_ratio() {
    let mut app = paused_app();
    // Pitch radii 0.6 and 1.2 (module 0.1): centres 1.8 apart.
    run(&mut app, "(spawn-gear 0 0 0.6 12)");
    run(&mut app, "(spawn-gear 1.8 0 1.2 24)");
    // Turn the big gear half a tooth so a gap faces the small gear's tooth.
    run(
        &mut app,
        &format!("(place 1 1.8 0 {})", std::f32::consts::PI / 24.0),
    );
    run(&mut app, "(begin (hinge 0 -1 0 0) (hinge 1 -1 1.8 0))");
    // Power the small gear's hinge at 1 rad/s.
    let small = bodies(&mut app)[0].1;
    let mut joints = app.world_mut().query::<(&StableId, &JointDef)>();
    let (joint_id, def) = joints
        .iter(app.world())
        .find(|(_, def)| def.body_a == small)
        .map(|(id, def)| (*id, def.clone()))
        .unwrap();
    let mut powered = def.clone();
    let JointKind::Hinge { motor, .. } = &mut powered.kind else {
        panic!("a hinge");
    };
    *motor = Some(AngularMotorDef {
        target_velocity: gradiance::units::AngularVelocity(1.0),
        ..default()
    });
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: joint_id,
            old: PropertyValue::Joint(def),
            new: PropertyValue::Joint(powered),
        }],
    });
    app.update();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    // One second to settle the mesh, then measure five.
    step(&mut app, 60);
    let ids = bodies(&mut app);
    let (small, big) = (ids[0].0, ids[1].0);
    let (mut sum_small, mut sum_big) = (0.0_f32, 0.0_f32);
    let frames = 300;
    for _ in 0..frames {
        step(&mut app, 1);
        sum_small += app.world().get::<AngularVelocity>(small).unwrap().0;
        sum_big += app.world().get::<AngularVelocity>(big).unwrap().0;
    }
    let (w_small, w_big) = (sum_small / frames as f32, sum_big / frames as f32);
    assert!(
        (w_small - 1.0).abs() < 0.05,
        "the motor drives the small gear at 1 rad/s: {w_small}"
    );
    assert!(
        (w_big + 0.5).abs() < 0.025,
        "the big gear turns at -0.5 rad/s: {w_big} (small {w_small})"
    );
}

/// `make-gear` turns a circle into a gear of the same pitch radius, in one
/// undo step, and leaves any other body alone.
#[test]
fn make_gear_turns_a_circle_into_a_gear_and_undoes() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-circle 0 0 0.6) (spawn-box 3 0 1 1))",
    );
    run(&mut app, "(begin (make-gear 0) (make-gear 1))");
    let shapes = bodies(&mut app);
    let ShapeDef::Polygon { outline, .. } = &shapes[0].2 else {
        panic!("the circle became a gear: {:?}", shapes[0].2);
    };
    assert_eq!(
        outline.len(),
        SAMPLES_PER_TOOTH * 12,
        "0.6 m at the default module: 12 teeth"
    );
    assert!(
        matches!(shapes[1].2, ShapeDef::Box { .. }),
        "a box stays a box"
    );
    undo(&mut app);
    assert!(matches!(bodies(&mut app)[0].2, ShapeDef::Circle { .. }));
    assert_eq!(
        body_count(&mut app),
        2,
        "undo reverted the shape, not the spawn"
    );
}
