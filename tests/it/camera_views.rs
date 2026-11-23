//! Camera follow and frame (brief S-9): `camera-follow` keeps a body at the
//! centre, `camera-frame` glides to show bodies, `camera-look` to a view.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, step};
use bevy::prelude::*;
use gradiance::interaction::camera::{CameraRig, FRAME_MARGIN};
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

/// A paused headless app with an orthographic editor camera at the start
/// zoom (headless apps spawn none; the viewport reads as 1600 x 900).
fn app_with_camera() -> App {
    let mut app = paused_app();
    app.world_mut().spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scale: 0.01,
            ..OrthographicProjection::default_3d()
        }),
        Transform::default(),
    ));
    app.update();
    app
}

fn world_per_pixel(app: &mut App) -> f32 {
    let mut cameras = app
        .world_mut()
        .query_filtered::<&Projection, With<Camera3d>>();
    match cameras.single(app.world()).unwrap() {
        Projection::Orthographic(o) => o.scale,
        other => panic!("orthographic: {other:?}"),
    }
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

/// The brief's test: follow keeps a falling body within 1 px of the
/// centre over 2 s.
#[test]
fn follow_keeps_a_falling_body_at_the_centre() {
    let mut app = app_with_camera();
    run(&mut app, "(spawn-circle 3 5 0.3)");
    run(&mut app, "(camera-follow 0)");
    let ball = bodies(&mut app)[0];
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    let start = app.world().get::<Transform>(ball).unwrap().translation;
    for _ in 0..120 {
        step(&mut app, 1);
        let at = app.world().get::<Transform>(ball).unwrap().translation;
        let focus = app.world().resource::<CameraRig>().focus;
        let px = world_per_pixel(&mut app);
        assert!(
            (at.truncate() - focus.truncate()).length() <= px,
            "within 1 px: body {at}, focus {focus}"
        );
    }
    let end = app.world().get::<Transform>(ball).unwrap().translation;
    assert!(start.y - end.y > 10.0, "it fell: {start} -> {end}");
    // -1 stops following.
    run(&mut app, "(camera-follow -1)");
    // The request lands on the next frame (the camera runs before the
    // operations).
    app.update();
    assert_eq!(app.world().resource::<CameraRig>().follow, None);
}

/// The brief's test: frame puts every target inside the viewport, and
/// fills it to the margin on one axis.
#[test]
fn frame_shows_every_target() {
    let mut app = app_with_camera();
    run(
        &mut app,
        "(begin (spawn-ground 0 -1 0) (spawn-box -30 4 2 2) (spawn-circle 25 -6 1) (spawn-box 0 40 1 1))",
    );
    run(&mut app, "(camera-frame (list 1 2))");
    step(&mut app, 120);
    let rig = *app.world().resource::<CameraRig>();
    let half = Vec2::new(800.0, 450.0) * world_per_pixel(&mut app);
    let inside = |p: Vec2| {
        let d = (p - rig.focus.truncate()).abs();
        d.x <= half.x && d.y <= half.y
    };
    for corner in [
        Vec2::new(-31.0, 3.0),
        Vec2::new(-29.0, 5.0),
        Vec2::new(24.0, -7.0),
        Vec2::new(26.0, -5.0),
    ] {
        assert!(inside(corner), "{corner} inside the view at {rig:?}");
    }
    // Width decides: 57 m across 80 % of the width.
    let expected = 57.0 / (1600.0 * (1.0 - 2.0 * FRAME_MARGIN));
    assert!((world_per_pixel(&mut app) - expected).abs() < 1e-4);
    assert!(!inside(Vec2::new(0.0, 40.0)), "an unnamed body may be out");
    // An empty list frames every body but the ground.
    run(&mut app, "(camera-frame (list))");
    step(&mut app, 120);
    let rig = *app.world().resource::<CameraRig>();
    let half = Vec2::new(800.0, 450.0) * world_per_pixel(&mut app);
    let d = (Vec2::new(0.0, 40.5) - rig.focus.truncate()).abs();
    assert!(d.x <= half.x && d.y <= half.y, "every body shows");
}

/// `camera-look` glides to a point and a zoom (pixels per metre).
#[test]
fn look_glides_to_a_point_and_a_zoom() {
    let mut app = app_with_camera();
    run(&mut app, "(camera-look 4 -2 50)");
    step(&mut app, 120);
    let rig = *app.world().resource::<CameraRig>();
    assert_eq!(rig.focus.truncate(), Vec2::new(4.0, -2.0));
    assert!((world_per_pixel(&mut app) - 0.02).abs() < 1e-6);
    assert_eq!(rig.view, None, "the glide ended");
}
