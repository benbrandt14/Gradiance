//! Authored initial velocity and spin (brief S-1): `set-velocity` /
//! `set-spin` author an `InitialMotion` (one undo step, saved); a run that
//! opens from an edited layout starts the body with it; a resumed run keeps
//! its live motion; undoing the run restores the pose and the motion; a
//! mirror turns the motion over.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, paused_app, step, undo};
use avian2d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::motion::InitialMotion;
use gradiance::domain::settings::SimSettings;
use gradiance::op::invoke;
use gradiance::prelude::*;
use gradiance::units::Acceleration2;
use serde_json::json;

/// A weightless paused app with one 1 m box at the origin.
fn one_box() -> (App, StableId) {
    let mut app = paused_app();
    app.world_mut().resource_mut::<SimSettings>().gravity = Acceleration2(Vec2::ZERO);
    let record = box_record(Vec2::ZERO, 1.0, 1.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(&mut app, 3);
    (app, id)
}

fn set_state(app: &mut App, state: GameState) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(state);
    app.update();
}

fn x_of(app: &App, id: StableId) -> f32 {
    app.world()
        .get::<Transform>(entity_of(app, id).unwrap())
        .unwrap()
        .translation
        .x
}

fn motion_of(app: &App, id: StableId) -> Option<InitialMotion> {
    app.world()
        .get::<InitialMotion>(entity_of(app, id).unwrap())
        .copied()
}

/// The brief's behaviour test: a body set to (3, 0) m/s has moved 3 m ± 2 %
/// after 1 s of play; undoing the run restores its pose and its motion.
#[test]
fn a_run_starts_the_body_with_its_velocity_and_undo_restores_it() {
    let (mut app, id) = one_box();
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    invoke(
        app.world_mut(),
        "body.set_velocity",
        json!({"i": 0, "vx": 3, "vy": 0}),
    )
    .unwrap();
    step(&mut app, 3);
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        depth + 1,
        "one undo step"
    );
    let authored = motion_of(&app, id).expect("authored");
    assert_eq!(authored.velocity.value(), Vec2::new(3.0, 0.0));

    set_state(&mut app, GameState::Playing);
    step(&mut app, 59);
    let moved = x_of(&app, id);
    assert!((moved - 3.0).abs() <= 0.06, "moved {moved} m in 1 s");

    set_state(&mut app, GameState::Paused);
    undo(&mut app);
    step(&mut app, 2);
    assert!(x_of(&app, id).abs() < 1e-4, "the pose is back");
    assert_eq!(motion_of(&app, id), Some(authored), "and the motion");
}

/// Resuming a paused run keeps the live motion: the authored velocity is
/// applied only when a run opens from an edited layout.
#[test]
fn a_resumed_run_keeps_its_live_motion() {
    let (mut app, id) = one_box();
    invoke(
        app.world_mut(),
        "body.set_velocity",
        json!({"i": 0, "vx": 3, "vy": 0}),
    )
    .unwrap();
    step(&mut app, 3);
    set_state(&mut app, GameState::Playing);
    step(&mut app, 20);
    set_state(&mut app, GameState::Paused);
    // Stop the body by hand while paused (a test-only write of derived
    // state): a resume must not launch it again.
    let entity = entity_of(&app, id).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .insert(LinearVelocity::ZERO);
    let x = x_of(&app, id);
    set_state(&mut app, GameState::Playing);
    step(&mut app, 30);
    assert!((x_of(&app, id) - x).abs() < 1e-3, "it stays put on resume");
}

/// Spin is authored the same way; a zero velocity and spin stores no
/// motion; the motion saves with the scene; a horizontal mirror reflects
/// the velocity and reverses the spin.
#[test]
fn spin_rest_save_and_mirror() {
    let (mut app, id) = one_box();
    // Each call lowers against the committed scene, so a frame between them.
    invoke(app.world_mut(), "body.set_spin", json!({"i": 0, "w": 2})).unwrap();
    step(&mut app, 3);
    invoke(
        app.world_mut(),
        "body.set_velocity",
        json!({"i": 0, "vx": 1, "vy": 2}),
    )
    .unwrap();
    step(&mut app, 3);
    let saved =
        gradiance::scene::to_ron(&gradiance::scene::SceneRecord::capture(app.world_mut())).unwrap();
    assert!(saved.contains("motion"), "saved: {saved}");

    invoke(app.world_mut(), "body.mirror_h", json!({"targets": [0]})).unwrap();
    step(&mut app, 3);
    let m = motion_of(&app, id).unwrap();
    assert!(
        (m.velocity.value() - Vec2::new(-1.0, 2.0)).length() < 1e-5,
        "{m:?}"
    );
    assert!((m.spin.value() + 2.0).abs() < 1e-5);

    set_state(&mut app, GameState::Playing);
    step(&mut app, 2);
    let w = app
        .world()
        .get::<AngularVelocity>(entity_of(&app, id).unwrap())
        .unwrap()
        .0;
    assert!((w + 2.0).abs() < 0.05, "the run starts with the spin ({w})");
    set_state(&mut app, GameState::Paused);

    invoke(
        app.world_mut(),
        "body.set_velocity",
        json!({"i": 0, "vx": 0, "vy": 0}),
    )
    .unwrap();
    step(&mut app, 3);
    invoke(app.world_mut(), "body.set_spin", json!({"i": 0, "w": 0})).unwrap();
    step(&mut app, 3);
    assert_eq!(motion_of(&app, id), None, "at rest is no motion");
}
