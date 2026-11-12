//! Rigid rod (brief 2-3): the rod tool authors a capsule body plus a hinge
//! at each end that lands on a body, as one undo step; the capsule persists.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used)]

use crate::harness::{body_count, box_record, joint_count, paused_app, redo, undo};
use avian2d::prelude::Collider;
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::scene::{from_ron, to_ron};

fn spawn_box_at(app: &mut App, pos: Vec2) -> StableId {
    let record = box_record(pos, 0.4, 0.4);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

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

/// Drags the rod tool from `a` to `b`.
fn draw_rod(app: &mut App, a: Vec2, b: Vec2) {
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Rod);
    // A realistic zoom (1 cm per pixel): headless defaults to 1 m per
    // pixel, which would make the screen-sized attach tolerance 8 m.
    app.world_mut()
        .insert_resource(gradiance::interaction::camera::CameraScale(0.01));
    app.update();
    set_cursor(app, a);
    mouse(app, true);
    app.update();
    set_cursor(app, b);
    app.update();
    mouse(app, false);
    app.update();
}

/// The capsule bodies in the world: `(shape, transform)`.
fn rods(app: &mut App) -> Vec<(ShapeDef, Transform)> {
    app.world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>()
        .iter(app.world())
        .filter(|(s, _)| matches!(s, ShapeDef::Capsule { .. }))
        .map(|(s, t)| (s.clone(), *t))
        .collect()
}

#[test]
fn a_rod_between_two_boxes_is_one_capsule_and_two_hinges_in_one_undo_step() {
    let mut app = paused_app();
    let left = spawn_box_at(&mut app, Vec2::ZERO);
    let right = spawn_box_at(&mut app, Vec2::new(2.0, 0.0));
    draw_rod(&mut app, Vec2::ZERO, Vec2::new(2.0, 0.0));

    assert_eq!(body_count(&mut app), 3, "two boxes + the rod");
    let rods = rods(&mut app);
    assert_eq!(rods.len(), 1);
    let (shape, transform) = &rods[0];
    let ShapeDef::Capsule { half_length, .. } = shape else {
        unreachable!()
    };
    assert!((half_length - 1.0).abs() < 1e-5, "spans the drag");
    assert!((transform.translation.truncate() - Vec2::new(1.0, 0.0)).length() < 1e-5);

    // Two hinges, rod to each box.
    let joints: Vec<JointDef> = app
        .world_mut()
        .query::<&JointDef>()
        .iter(app.world())
        .cloned()
        .collect();
    assert_eq!(joints.len(), 2);
    for j in &joints {
        assert!(matches!(j.kind, JointKind::Hinge { .. }), "{:?}", j.kind);
    }
    let others: Vec<_> = joints.iter().filter_map(|j| j.body_b).collect();
    assert!(
        others.len() == 2 && others.contains(&left) && others.contains(&right),
        "one hinge per box"
    );

    // The collider is derived (an exact capsule), so the rod simulates.
    app.update();
    let mut colliders = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Collider), With<Body>>();
    assert!(
        colliders
            .iter(app.world())
            .any(|(s, c)| matches!(s, ShapeDef::Capsule { .. })
                && c.shape().as_capsule().is_some()),
        "a capsule collider"
    );

    undo(&mut app);
    assert_eq!(body_count(&mut app), 2, "one undo removes the rod");
    assert_eq!(joint_count(&mut app), 0, "…and both hinges");
    redo(&mut app);
    assert_eq!(body_count(&mut app), 3);
    assert_eq!(joint_count(&mut app), 2);
}

#[test]
fn a_rod_over_empty_space_has_free_ends() {
    let mut app = paused_app();
    draw_rod(&mut app, Vec2::new(5.0, 5.0), Vec2::new(6.0, 6.0));
    assert_eq!(rods(&mut app).len(), 1);
    assert_eq!(joint_count(&mut app), 0, "no body under either end");

    // One end on a box, the other free: one hinge.
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::ZERO);
    draw_rod(&mut app, Vec2::ZERO, Vec2::new(0.0, 3.0));
    assert_eq!(joint_count(&mut app), 1);
}

#[test]
fn a_too_short_rod_commits_nothing() {
    let mut app = paused_app();
    draw_rod(&mut app, Vec2::ZERO, Vec2::new(0.001, 0.0));
    assert_eq!(body_count(&mut app), 0);
}

#[test]
fn a_rod_scene_round_trips_through_ron() {
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::ZERO);
    draw_rod(&mut app, Vec2::ZERO, Vec2::new(1.5, 0.5));
    let scene = SceneRecord::capture(app.world_mut());
    let text = to_ron(&scene).unwrap();
    assert!(text.contains("Capsule"), "the leaf is saved as authored");
    let parsed = from_ron(&text).unwrap();
    assert_eq!(parsed.bodies, scene.bodies);
    assert_eq!(parsed.joints, scene.joints);
}
