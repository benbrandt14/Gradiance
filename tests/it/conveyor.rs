//! Conveyor surfaces (brief S-8): a body whose surface runs along its
//! outline carries what rests on it, while the body itself stays put.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, step, undo};
use avian2d::prelude::LinearVelocity;
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

/// Every body, in id (spawn) order.
fn bodies(app: &mut App) -> Vec<Entity> {
    let mut query = app
        .world_mut()
        .query_filtered::<(Entity, &StableId), With<Body>>();
    let mut out: Vec<_> = query.iter(app.world()).map(|(e, id)| (e, *id)).collect();
    out.sort_by_key(|(_, id)| id.0);
    out.into_iter().map(|(e, _)| e).collect()
}

/// The load's mean x velocity over one second, after two to settle, on a
/// static belt whose surface runs at `speed`.
fn carried_speed(speed: f32) -> (f32, Vec3, Vec3) {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 20 0.2) (spawn-box -5 0.4 0.5 0.5))",
    );
    run(
        &mut app,
        &format!("(begin (set-static 0 1) (set-surface-speed 0 {speed}))"),
    );
    let [belt, load] = bodies(&mut app)[..] else {
        panic!("two bodies");
    };
    let belt_start = app.world().get::<Transform>(belt).unwrap().translation;
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 120);
    let mut sum = 0.0;
    for _ in 0..60 {
        step(&mut app, 1);
        sum += app.world().get::<LinearVelocity>(load).unwrap().x;
    }
    let belt_end = app.world().get::<Transform>(belt).unwrap().translation;
    (sum / 60.0, belt_start, belt_end)
}

/// The brief's behaviour test: a box on a 1 m/s static conveyor reaches
/// 1 m/s (+-5 %). A negative speed runs the top clockwise: to the right.
#[test]
fn a_box_on_a_conveyor_reaches_the_belt_speed() {
    let (v, belt_start, belt_end) = carried_speed(-1.0);
    assert!((v - 1.0).abs() <= 0.05, "carried right at 1 m/s: {v}");
    assert_eq!(belt_start, belt_end, "the belt itself stays put");
    let (v, _, _) = carried_speed(2.0);
    assert!((v + 2.0).abs() <= 0.1, "carried left at 2 m/s: {v}");
}

/// `set-surface-speed` is one undo step; 0 stops the surface.
#[test]
fn set_surface_speed_undoes_and_zero_clears() {
    use gradiance::domain::surface::SurfaceSpeed;
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 4 0.2)");
    run(&mut app, "(set-surface-speed 0 1.5)");
    let belt = bodies(&mut app)[0];
    assert_eq!(
        app.world().get::<SurfaceSpeed>(belt).map(|s| s.0.0),
        Some(1.5)
    );
    run(&mut app, "(set-surface-speed 0 0)");
    assert!(app.world().get::<SurfaceSpeed>(belt).is_none(), "0 clears");
    undo(&mut app);
    assert_eq!(
        app.world().get::<SurfaceSpeed>(belt).map(|s| s.0.0),
        Some(1.5)
    );
}
