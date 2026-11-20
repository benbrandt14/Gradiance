//! Sketch tool (brief N-6): a freehand stroke becomes one body, its
//! simplified segments capsules joined as one SDF union, or a filled polygon
//! when it closes; one undo step; `polygonize` makes the collider.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used)]

use crate::harness::{body_count, headless_app, paused_app, redo, step, undo};
use avian2d::prelude::{Collider, LinearVelocity};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::settings::ToolDefaults;
use gradiance::geometry::sketch::{MAX_SKETCH_SEGMENTS, leaf_count};
use gradiance::op::{InvokeOp, LastOp, Value, json};
use gradiance::prelude::*;
use gradiance::units::Length;

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn mouse(app: &mut App, down: bool) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    if down {
        input.press(MouseButton::Left);
    } else {
        input.release(MouseButton::Left);
    }
}

/// Drags the sketch tool through `path` (press at the first point, a frame
/// per sample, release at the last), then lets the invocation it commits
/// land.
fn draw(app: &mut App, path: &[Vec2]) {
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Sketch);
    // A realistic zoom (1 cm per pixel), as in the rod and chain tests.
    app.world_mut()
        .insert_resource(gradiance::interaction::camera::CameraScale(0.01));
    app.update();
    set_cursor(app, path[0]);
    mouse(app, true);
    app.update();
    for p in &path[1..] {
        set_cursor(app, *p);
        app.update();
    }
    mouse(app, false);
    app.update();
    app.update();
}

fn invoke(app: &mut App, id: &'static str, params: Value) {
    app.world_mut().write_message(InvokeOp::with(id, params));
    app.update();
    app.update();
}

fn depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

/// Every body's shape and pose.
fn shapes(app: &mut App) -> Vec<(ShapeDef, Transform)> {
    app.world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>()
        .iter(app.world())
        .map(|(s, t)| (s.clone(), *t))
        .collect()
}

/// A wave of `n` samples across `span` metres, starting at `at`.
fn wave(n: usize, at: Vec2, span: f32, amplitude: f32) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            at + Vec2::new(
                span * t,
                amplitude * (t * std::f32::consts::TAU * 1.5).sin(),
            )
        })
        .collect()
}

/// A loop of `n` samples about `centre`, ending just short of its start.
fn ring(n: usize, centre: Vec2, r: f32) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let a = (std::f32::consts::TAU - 0.15) * i as f32 / (n - 1) as f32;
            centre + r * Vec2::new(a.cos(), a.sin())
        })
        .collect()
}

#[test]
fn a_stroke_of_200_samples_is_one_body_of_under_30_leaves_and_one_undo_step() {
    let mut app = paused_app();
    let before = depth(&app);
    // 6 m across: the samples land 3-4 cm apart, past the tool's 3 px step.
    let stroke = wave(200, Vec2::new(-3.0, 2.0), 6.0, 1.0);
    draw(&mut app, &stroke);

    assert_eq!(body_count(&mut app), 1, "one body");
    let (shape, pose) = shapes(&mut app).pop().unwrap();
    assert!(matches!(shape, ShapeDef::Csg { .. }), "a union: {shape:?}");
    let leaves = leaf_count(&shape);
    assert!(
        (2..30).contains(&leaves) && leaves <= MAX_SKETCH_SEGMENTS,
        "{leaves} leaves"
    );
    assert!(shape.depth() <= gradiance::geometry::shape::MAX_CSG_DEPTH);
    // The body covers the stroke it was drawn from.
    let inverse = pose.compute_affine().inverse();
    for p in &stroke {
        let local = inverse.transform_point3(p.extend(0.0)).truncate();
        assert!(
            gradiance::geometry::sdf::eval(&shape, local) < 0.03,
            "{p} is off the body"
        );
    }
    // `polygonize` made the collider: one piece, there in the world.
    step(&mut app, 2);
    let colliders = app
        .world_mut()
        .query_filtered::<&Collider, With<Body>>()
        .iter(app.world())
        .count();
    assert_eq!(colliders, 1);
    let parts = gradiance::geometry::polygonize::polygonize_components(&shape);
    assert_eq!(parts.len(), 1, "one connected piece");

    assert_eq!(depth(&app), before + 1, "one gesture, one step");
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_label,
        Some(gradiance::command::intent::name::SPAWN_BODY)
    );
    let last = app
        .world()
        .resource::<LastOp>()
        .0
        .as_ref()
        .map(|r| r.call.id);
    assert_eq!(last, Some("body.sketch"), "the gesture was the operation");

    undo(&mut app);
    assert_eq!(body_count(&mut app), 0, "one undo removes it");
    redo(&mut app);
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(shapes(&mut app).pop().unwrap().0, shape, "redo restores it");
}

#[test]
fn a_closed_stroke_fills_as_a_polygon() {
    let mut app = paused_app();
    let centre = Vec2::new(1.0, 2.0);
    draw(&mut app, &ring(120, centre, 0.6));
    let (shape, pose) = shapes(&mut app).pop().unwrap();
    let ShapeDef::Polygon { outline, holes } = &shape else {
        panic!("a polygon: {shape:?}");
    };
    assert!(holes.is_empty());
    assert!(outline.len() >= 8);
    assert!((pose.translation.truncate() - centre).length() < 0.03);
    let area = gradiance::geometry::contours::ring_signed_area(outline);
    let ideal = std::f32::consts::PI * 0.36;
    assert!((area - ideal).abs() / ideal < 0.08, "{area} vs {ideal}");
}

#[test]
fn a_straight_stroke_is_one_capsule() {
    let mut app = paused_app();
    let stroke: Vec<Vec2> = (0..=20).map(|k| Vec2::new(0.1 * k as f32, 1.0)).collect();
    draw(&mut app, &stroke);
    let (shape, pose) = shapes(&mut app).pop().unwrap();
    let ShapeDef::Capsule {
        half_length,
        radius,
    } = shape
    else {
        panic!("a capsule: {shape:?}");
    };
    assert!((half_length - 1.0).abs() < 1e-4);
    assert!((radius - 0.05).abs() < 1e-6, "half the default 10 cm width");
    assert!((pose.translation.truncate() - Vec2::new(1.0, 1.0)).length() < 1e-4);
}

#[test]
fn the_width_comes_from_the_tool_defaults_and_adjusts_after() {
    let mut app = paused_app();
    app.world_mut().resource_mut::<ToolDefaults>().sketch_width = Length(0.2);
    draw(&mut app, &wave(60, Vec2::ZERO, 2.0, 0.4));
    let radius_of = |shape: &ShapeDef| {
        let mut found = None;
        let mut node = shape;
        loop {
            match node {
                ShapeDef::Csg { lhs, .. } => node = lhs,
                ShapeDef::Placed { shape, .. } => node = shape,
                ShapeDef::Capsule { radius, .. } => {
                    found = Some(*radius);
                    break;
                }
                _ => break,
            }
        }
        found
    };
    let shape = shapes(&mut app).pop().unwrap().0;
    assert!((radius_of(&shape).unwrap() - 0.1).abs() < 1e-6);
    let steps = depth(&app);
    invoke(
        &mut app,
        "edit.adjust_last",
        json!({"field": "width", "value": 0.06}),
    );
    assert_eq!(body_count(&mut app), 1, "re-made in place");
    let shape = shapes(&mut app).pop().unwrap().0;
    assert!((radius_of(&shape).unwrap() - 0.03).abs() < 1e-6);
    assert_eq!(depth(&app), steps, "the adjustment replaces the step");
}

#[test]
fn a_click_makes_no_body() {
    let mut app = paused_app();
    draw(&mut app, &[Vec2::ZERO, Vec2::new(0.005, 0.0)]);
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn the_operation_refuses_a_bad_path_or_width() {
    let mut app = paused_app();
    let call = |app: &mut App, params: Value| {
        gradiance::op::invoke(app.world_mut(), "body.sketch", params)
    };
    let ok = json!({"path": [0, 0, 0.5, 0.3, 1, 0], "width": 0.1});
    assert!(call(&mut app, ok.clone()).is_ok());
    for (key, bad) in [
        ("path", json!([0, 0, 1])),
        ("path", json!([0, 0])),
        ("path", json!([0, 0, 0.001, 0])),
        ("width", json!(0)),
        ("width", json!(-1)),
    ] {
        let mut params = ok.clone();
        params[key] = bad;
        assert!(call(&mut app, params).is_err(), "{key}");
    }
}

/// A sketched hook dropped on the ground lands and settles on it in one
/// piece: the contoured collider holds up in the solver.
#[test]
fn a_sketched_body_falls_and_rests_on_the_ground() {
    let mut app = headless_app();
    invoke(
        &mut app,
        "body.spawn_ground",
        json!({"x": 0, "y": 0, "angle": 0}),
    );
    // A J: down, round the bottom, and up a little.
    let mut stroke: Vec<Vec2> = (0..=10)
        .map(|k| Vec2::new(0.0, 2.0 - 0.1 * k as f32))
        .collect();
    stroke.extend((1..=12).map(|k| {
        let a = std::f32::consts::PI * (1.0 + k as f32 / 12.0);
        Vec2::new(0.3, 1.0) + 0.3 * Vec2::new(a.cos(), a.sin())
    }));
    invoke(
        &mut app,
        "body.sketch",
        json!({"path": stroke.iter().flat_map(|p| [p.x, p.y]).collect::<Vec<f32>>(), "width": 0.08}),
    );
    assert_eq!(body_count(&mut app), 2);
    step(&mut app, 240);
    let world = app.world_mut();
    let mut query = world.query_filtered::<(&ShapeDef, &Transform, &LinearVelocity), With<Body>>();
    let (shape, pose, velocity) = query
        .iter(world)
        .find(|(s, _, _)| !s.contains_half_plane())
        .map(|(s, t, v)| (s.clone(), *t, v.0))
        .unwrap();
    assert!(pose.translation.is_finite());
    let affine = pose.compute_affine();
    let lowest = gradiance::geometry::polygonize::polygonize(&shape)
        .outline
        .iter()
        .map(|v| affine.transform_point3(v.extend(0.0)).y)
        .fold(f32::INFINITY, f32::min);
    assert!(lowest > -0.03, "sank into the ground: {lowest}");
    assert!(lowest < 0.05, "never landed: {lowest}");
    assert!(velocity.length() < 0.2, "still moving at {velocity}");
}
