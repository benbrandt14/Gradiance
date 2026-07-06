//! Tool-layer tests: gestures drive exactly one command; scale and array
//! commands behave exactly and undo cleanly.

mod harness;

use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::prelude::*;
use harness::{body_count, box_record, entity_of, paused_app, undo};

fn stack_undo_len(app: &App) -> usize {
    app.world().resource::<CommandStack>().undo_len()
}

fn spawn_box_at(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    let record = box_record(pos, w, h);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    // SnappedCursor normally updates in PreUpdate from CursorWorldPos.
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn mouse(app: &mut App, button: MouseButton, down: bool) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    if down {
        input.press(button);
    } else {
        input.release(button);
    }
}

// ---------- Scale command ----------

#[test]
fn scale_command_scales_shape_and_position_and_undoes() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::new(10.0, 0.0), 40.0, 20.0);

    app.world_mut().write_message(ScaleIntent {
        targets: vec![id],
        pivot: Vec2::ZERO,
        frame_rot: 0.0,
        factors: Vec2::new(2.0, 1.0),
    });
    app.update();

    let entity = entity_of(&app, id).unwrap();
    let shape = app.world().get::<ShapeDef>(entity).unwrap().clone();
    assert_eq!(
        shape,
        ShapeDef::Box {
            width: 80.0,
            height: 20.0
        }
    );
    let pos = app
        .world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate();
    assert!((pos - Vec2::new(20.0, 0.0)).length() < 1e-4);

    undo(&mut app);
    let shape = app.world().get::<ShapeDef>(entity).unwrap().clone();
    assert_eq!(
        shape,
        ShapeDef::Box {
            width: 40.0,
            height: 20.0
        }
    );
    let pos = app
        .world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate();
    assert!((pos - Vec2::new(10.0, 0.0)).length() < 1e-4);
}

#[test]
fn non_uniform_scale_of_rotated_box_polygonizes_exactly() {
    let mut app = paused_app();
    let mut record = box_record(Vec2::ZERO, 40.0, 20.0);
    record.pose.rot = 30_f32.to_radians();
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    app.world_mut().write_message(ScaleIntent {
        targets: vec![id],
        pivot: Vec2::ZERO,
        frame_rot: 0.0, // global axes, body is rotated → not representable as a box
        factors: Vec2::new(2.0, 1.0),
    });
    app.update();

    let entity = entity_of(&app, id).unwrap();
    let shape = app.world().get::<ShapeDef>(entity).unwrap().clone();
    let ShapeDef::Polygon { outline, .. } = &shape else {
        panic!("expected exact polygonization, got {shape:?}");
    };
    let area = gradiance::geometry::contours::ring_signed_area(outline).abs();
    assert!((area - 1600.0).abs() < 1.0, "area doubled exactly ({area})");
}

#[test]
fn degenerate_scale_factors_are_refused() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 20.0);
    let before = stack_undo_len(&app);

    app.world_mut().write_message(ScaleIntent {
        targets: vec![id],
        pivot: Vec2::ZERO,
        frame_rot: 0.0,
        factors: Vec2::new(0.0, 1.0),
    });
    app.update();

    assert_eq!(stack_undo_len(&app), before, "refused command not recorded");
    let entity = entity_of(&app, id).unwrap();
    assert_eq!(
        app.world().get::<ShapeDef>(entity).unwrap().clone(),
        ShapeDef::Box {
            width: 40.0,
            height: 20.0
        }
    );
}

// ---------- Array command ----------

#[test]
fn linear_array_creates_offset_copies_in_one_undo_step() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 20.0, 20.0);

    app.world_mut().write_message(ArrayIntent {
        sources: vec![id],
        count: 3,
        mode: ArrayMode::Linear {
            offset: Vec2::new(50.0, 0.0),
        },
    });
    app.update();
    assert_eq!(body_count(&mut app), 4);

    let mut xs: Vec<f32> = {
        let mut q = app.world_mut().query_filtered::<&Transform, With<Body>>();
        q.iter(app.world()).map(|t| t.translation.x).collect()
    };
    xs.sort_by(f32::total_cmp);
    assert_eq!(xs, vec![0.0, 50.0, 100.0, 150.0]);

    undo(&mut app);
    assert_eq!(body_count(&mut app), 1, "whole array is one undo step");
}

#[test]
fn radial_array_places_copies_on_the_circle() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::new(100.0, 0.0), 20.0, 20.0);

    app.world_mut().write_message(ArrayIntent {
        sources: vec![id],
        count: 3,
        mode: ArrayMode::Radial {
            pivot: Vec2::ZERO,
            angle_step: std::f32::consts::FRAC_PI_2,
            rotate_items: true,
        },
    });
    app.update();
    assert_eq!(body_count(&mut app), 4);

    let positions: Vec<Vec2> = {
        let mut q = app.world_mut().query_filtered::<&Transform, With<Body>>();
        q.iter(app.world())
            .map(|t| t.translation.truncate())
            .collect()
    };
    for p in &positions {
        assert!((p.length() - 100.0).abs() < 1e-3, "on the ring: {p}");
    }
    // One copy per quadrant axis.
    assert!(positions.iter().any(|p| p.y > 99.0));
    assert!(positions.iter().any(|p| p.x < -99.0));
}

// ---------- Select tool gestures ----------

#[test]
fn click_selects_and_drag_commits_one_move_command() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 20.0);
    app.update(); // colliders

    // Press on the body, drag right 100, release.
    set_cursor(&mut app, Vec2::new(5.0, 0.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    assert!(
        app.world()
            .resource::<Selection>()
            .contains(entity_of(&app, id).unwrap()),
        "press selects the hit body"
    );
    set_cursor(&mut app, Vec2::new(105.0, 0.0));
    app.update();
    let before = stack_undo_len(&app);
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(stack_undo_len(&app), before + 1, "one command per gesture");
    let entity = entity_of(&app, id).unwrap();
    let pos = app
        .world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate();
    // Grab-by-snap-point semantics: the press at (5,0) snapped to the body
    // center, so the center lands exactly on the release cursor. Mid-drag
    // the body itself is excluded from snapping (SnapExclusions).
    assert!((pos - Vec2::new(105.0, 0.0)).length() < 1.0, "{pos}");

    undo(&mut app);
    let pos = app
        .world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate();
    assert!(pos.length() < 1.0, "undo restores origin ({pos})");
}

#[test]
fn ctrl_drag_duplicates_with_offset() {
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::ZERO, 40.0, 20.0);
    app.update();

    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ControlLeft);
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(80.0, 0.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::ControlLeft);

    assert_eq!(body_count(&mut app), 2, "ctrl-drag made a copy");
    undo(&mut app);
    assert_eq!(body_count(&mut app), 1, "duplicate is one undo step");
}

#[test]
fn box_select_on_empty_canvas_selects_contained_bodies() {
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::new(0.0, 0.0), 20.0, 20.0);
    spawn_box_at(&mut app, Vec2::new(60.0, 0.0), 20.0, 20.0);
    spawn_box_at(&mut app, Vec2::new(500.0, 500.0), 20.0, 20.0);
    app.update();

    set_cursor(&mut app, Vec2::new(-40.0, -40.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(100.0, 40.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(app.world().resource::<Selection>().len(), 2);
}

// ---------- Creation tools ----------

#[test]
fn box_tool_drag_spawns_a_box() {
    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Box);
    app.update();

    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(60.0, 40.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(body_count(&mut app), 1);
    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    let (shape, transform) = q.iter(app.world()).next().unwrap();
    assert_eq!(
        shape.clone(),
        ShapeDef::Box {
            width: 60.0,
            height: 40.0
        }
    );
    assert!(
        (transform.translation.truncate() - Vec2::new(30.0, 20.0)).length() < 1e-3,
        "spawned at drag center"
    );
}

#[test]
fn polygon_tool_clicks_close_into_a_ccw_centroid_relative_polygon() {
    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Polygon);
    app.update();

    for p in [
        Vec2::new(0.0, 0.0),
        Vec2::new(60.0, 0.0),
        Vec2::new(30.0, 60.0),
    ] {
        set_cursor(&mut app, p);
        mouse(&mut app, MouseButton::Left, true);
        app.update();
        mouse(&mut app, MouseButton::Left, false);
        app.update();
    }
    // Close by clicking near the first vertex.
    set_cursor(&mut app, Vec2::new(2.0, 2.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(body_count(&mut app), 1);
    let mut q = app.world_mut().query_filtered::<&ShapeDef, With<Body>>();
    let ShapeDef::Polygon { outline, .. } = q.iter(app.world()).next().unwrap().clone() else {
        panic!("expected polygon");
    };
    assert_eq!(outline.len(), 3);
    assert!(gradiance::geometry::contours::ring_signed_area(&outline) > 0.0);
    let centroid: Vec2 = gradiance::geometry::contours::ring_centroid(&outline);
    assert!(centroid.length() < 1e-3, "centroid-relative vertices");
}

#[test]
fn ground_tool_spawns_static_half_plane_with_drag_tilt() {
    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Ground);
    app.update();

    set_cursor(&mut app, Vec2::new(0.0, -50.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(100.0, -20.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &PhysicalProps, &Transform), With<Body>>();
    let (shape, props, transform) = q.iter(app.world()).next().expect("ground spawned");
    assert_eq!(*shape, ShapeDef::HalfPlane);
    assert_eq!(props.body, BodyKind::Static);
    let expected = (Vec2::new(100.0, -20.0) - Vec2::new(0.0, -50.0)).to_angle();
    let rot = PosRot::from_transform(transform).rot;
    assert!((rot - expected).abs() < 1e-3, "tilt follows drag ({rot})");
}

// ---------- Right-click contract (context menu path) ----------

/// A right *click* (no drag) on a selected body must not move anything
/// and must not create an undo entry — the click belongs to the context
/// menu. A right *drag* still rotates.
#[test]
fn right_click_on_selection_commits_nothing() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    let entity = entity_of(&app, id).unwrap();
    app.world_mut().resource_mut::<Selection>().set(entity);

    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, MouseButton::Right, true);
    app.update();
    mouse(&mut app, MouseButton::Right, false);
    app.update();

    assert_eq!(stack_undo_len(&app), 1, "only the spawn is recorded");
    let pose = app.world().get::<Transform>(entity).unwrap();
    assert!(pose.translation.truncate().length() < 1e-3);
    assert!((pose.rotation.to_euler(EulerRot::ZYX).0).abs() < 1e-4);
}

/// Dragging right past the deadzone still rotates and commits one step.
#[test]
fn right_drag_past_deadzone_still_rotates() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 40.0);
    let entity = entity_of(&app, id).unwrap();
    app.world_mut().resource_mut::<Selection>().set(entity);

    // Grab to the right of the pivot, sweep up 90°.
    set_cursor(&mut app, Vec2::new(20.0, 0.0));
    mouse(&mut app, MouseButton::Right, true);
    app.update();
    set_cursor(&mut app, Vec2::new(0.0, 20.0));
    app.update();
    mouse(&mut app, MouseButton::Right, false);
    app.update();

    assert_eq!(stack_undo_len(&app), 2, "rotation committed one step");
    let rot = PosRot::from_transform(app.world().get::<Transform>(entity).unwrap()).rot;
    assert!(
        (rot - std::f32::consts::FRAC_PI_2).abs() < 0.05,
        "quarter turn, got {rot}"
    );
}
