//! Trackpad and touch camera gestures (brief S-21): two fingers pan, zoom
//! and roll the view; a trackpad's pixel scroll pans and its pinch zooms;
//! a wheel's line scroll still zooms. Camera only: no command, no undo
//! step.

#![allow(clippy::unwrap_used)]

use crate::harness::paused_app;
use bevy::input::gestures::{PinchGesture, RotationGesture};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::interaction::camera::CameraRig;

/// A paused headless app with an orthographic editor camera at 0.01 m per
/// pixel (headless apps spawn none; the viewport reads as 1600 x 900).
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

fn rig(app: &App) -> CameraRig {
    *app.world().resource::<CameraRig>()
}

fn touch(app: &mut App, id: u64, phase: TouchPhase, at: Vec2) {
    app.world_mut().write_message(TouchInput {
        phase,
        position: at,
        window: Entity::PLACEHOLDER,
        force: None,
        id,
    });
}

/// Puts two fingers at `a` and `b` (started or moved) and runs a frame.
fn fingers(app: &mut App, phase: TouchPhase, a: Vec2, b: Vec2) {
    touch(app, 0, phase, a);
    touch(app, 1, phase, b);
    app.update();
}

fn scroll(app: &mut App, unit: MouseScrollUnit, y: f32) {
    app.world_mut().write_message(MouseWheel {
        unit,
        x: 0.0,
        y,
        window: Entity::PLACEHOLDER,
        phase: TouchPhase::Moved,
    });
    app.update();
}

/// The brief's test: injected gestures move the rig as expected.
#[test]
fn touch_and_trackpad_gestures_pan_zoom_and_roll_the_camera() {
    let mut app = app_with_camera();
    let depth = app.world().resource::<HistoryInfo>().undo_depth;

    // Two fingers drag 50 px right: the scene moves with them, so the
    // focus moves 0.5 m left. Fingers at rest do not move it again.
    fingers(
        &mut app,
        TouchPhase::Started,
        Vec2::new(700.0, 450.0),
        Vec2::new(900.0, 450.0),
    );
    assert_eq!(rig(&app).focus, Vec3::ZERO, "a touch alone does not pan");
    fingers(
        &mut app,
        TouchPhase::Moved,
        Vec2::new(750.0, 450.0),
        Vec2::new(950.0, 450.0),
    );
    let focus = rig(&app).focus;
    assert!(
        (focus.x + 0.5).abs() < 1e-4 && focus.y.abs() < 1e-4,
        "pan: {focus}"
    );
    app.update();
    app.update();
    assert_eq!(rig(&app).focus, focus, "fingers at rest hold the view");

    // Spread twice as wide: twice the zoom.
    fingers(
        &mut app,
        TouchPhase::Moved,
        Vec2::new(650.0, 450.0),
        Vec2::new(1050.0, 450.0),
    );
    assert!(
        (world_per_pixel(&mut app) - 0.005).abs() < 1e-6,
        "pinch out zooms in"
    );

    // Turn the pair 45 degrees counter-clockwise: the view rolls the
    // other way, so the scene turns with the fingers.
    fingers(
        &mut app,
        TouchPhase::Moved,
        Vec2::new(708.6, 591.4),
        Vec2::new(991.4, 308.6),
    );
    let roll = rig(&app).roll;
    assert!(
        (roll + std::f32::consts::FRAC_PI_4).abs() < 1e-3,
        "roll: {roll}"
    );
    fingers(
        &mut app,
        TouchPhase::Ended,
        Vec2::new(708.6, 591.4),
        Vec2::new(991.4, 308.6),
    );
    app.world_mut().resource_mut::<CameraRig>().roll = 0.0;
    app.update();

    // A trackpad scrolls in pixels: 30 px down pans 30 px, not a zoom.
    let before = rig(&app).focus;
    let wpp = world_per_pixel(&mut app);
    scroll(&mut app, MouseScrollUnit::Pixel, 30.0);
    let moved = rig(&app).focus - before;
    assert!(
        (moved.y - 30.0 * wpp).abs() < 1e-5 && moved.x.abs() < 1e-5,
        "{moved}"
    );
    assert_eq!(world_per_pixel(&mut app), wpp, "pixel scroll does not zoom");

    // A wheel scrolls in lines and still zooms.
    scroll(&mut app, MouseScrollUnit::Line, 1.0);
    assert!(world_per_pixel(&mut app) < wpp, "a wheel notch zooms in");

    // Trackpad pinch and rotate messages.
    let wpp = world_per_pixel(&mut app);
    app.world_mut().write_message(PinchGesture(1.0));
    app.update();
    assert!(
        (world_per_pixel(&mut app) - wpp / 2.0).abs() < 1e-7,
        "pinch zooms in"
    );
    app.world_mut().write_message(RotationGesture(0.3));
    app.update();
    assert!((rig(&app).roll + 0.3).abs() < 1e-5, "rotate rolls the view");

    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        depth,
        "camera gestures record no undo step"
    );
}
