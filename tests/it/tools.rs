//! Tool-layer tests: gestures drive exactly one command; scale and array
//! commands behave exactly and undo cleanly.

use crate::harness::{body_count, box_record, entity_of, paused_app, step, undo};
use avian2d::prelude::{AngularVelocity, RigidBody};
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::command::array_cmd::ArrayTweens;
use gradiance::physics::grab::MouseTwist;
use gradiance::prelude::*;

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
    let entity = entity_of(&app, id).unwrap();
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
            step: Vec2::new(50.0, 0.0),
            ratio: 1.0,
            axis_y: false,
        },
        tweens: ArrayTweens::default(),
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
        tweens: ArrayTweens::default(),
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
    let entity = entity_of(&app, id).unwrap();
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
        Vec2::new(0.6, 0.0),
        Vec2::new(0.3, 0.6),
    ] {
        set_cursor(&mut app, p);
        mouse(&mut app, MouseButton::Left, true);
        app.update();
        mouse(&mut app, MouseButton::Left, false);
        app.update();
    }
    // Close by clicking near the first vertex.
    set_cursor(&mut app, Vec2::new(0.02, 0.02));
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
        .query_filtered::<(&ShapeDef, &RigidBody, &Transform), With<Body>>();
    let (shape, props, transform) = q.iter(app.world()).next().expect("ground spawned");
    assert_eq!(*shape, ShapeDef::HalfPlane);
    assert_eq!(*props, RigidBody::Static);
    let expected = (Vec2::new(100.0, -20.0) - Vec2::new(0.0, -50.0)).to_angle();
    let rot = PosRot::from_transform(transform).rot;
    assert!((rot - expected).abs() < 1e-3, "tilt follows drag ({rot})");
}

// ---------- Click-through selection (any tool selects on a plain click) ----------

#[test]
fn click_with_box_tool_selects_instead_of_spawning() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 20.0);
    app.update(); // colliders
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Box);
    app.update();

    // A plain click (no drag) on the body: the box tool commits nothing
    // (sub-minimum size) and the click falls through to selection.
    set_cursor(&mut app, Vec2::new(5.0, 0.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(body_count(&mut app), 1, "no body spawned by the click");
    assert!(
        app.world()
            .resource::<Selection>()
            .contains(entity_of(&app, id).unwrap()),
        "click fell through to select the hit body"
    );

    // A plain click on empty canvas clears the selection, still in Box mode.
    set_cursor(&mut app, Vec2::new(500.0, 500.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();
    assert!(app.world().resource::<Selection>().is_empty());
    assert_eq!(body_count(&mut app), 1, "empty click spawned nothing");
}

#[test]
fn box_tool_drag_still_spawns_and_does_not_reselect() {
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::new(300.0, 300.0), 40.0, 20.0);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Box);
    app.update();

    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(60.0, 40.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(body_count(&mut app), 2, "the drag authored a box");
    assert!(
        app.world().resource::<Selection>().is_empty(),
        "a committed draft never doubles as a selection click"
    );
}

// ---------- Shift semantics (toggle click / additive band, never a move) ----------

#[test]
fn shift_click_toggles_selection_without_moving() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 40.0, 20.0);
    app.update();
    let entity = entity_of(&app, id).unwrap();
    let before = stack_undo_len(&app);

    let shift_click = |app: &mut App| {
        set_cursor(app, Vec2::new(5.0, 0.0));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        mouse(app, MouseButton::Left, true);
        app.update();
        mouse(app, MouseButton::Left, false);
        app.update();
    };
    shift_click(&mut app);
    assert!(
        app.world().resource::<Selection>().contains(entity),
        "shift-click toggles the body in"
    );
    shift_click(&mut app);
    assert!(
        !app.world().resource::<Selection>().contains(entity),
        "second shift-click toggles it back out"
    );

    assert_eq!(stack_undo_len(&app), before, "no command committed");
    let pos = app
        .world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate();
    assert!(pos.length() < 1e-3, "shift never moves the body ({pos})");
}

#[test]
fn shift_drag_from_a_body_becomes_an_additive_band() {
    let mut app = paused_app();
    // A is large so the press point (10,10) is beyond the 12px snap capture
    // of any of its vertices/edges/center — the press lands where aimed.
    let a = spawn_box_at(&mut app, Vec2::ZERO, 100.0, 100.0);
    let b = spawn_box_at(&mut app, Vec2::new(200.0, 0.0), 40.0, 20.0);
    app.update();
    let (ea, eb) = (entity_of(&app, a).unwrap(), entity_of(&app, b).unwrap());

    // Select A with a plain click.
    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();
    assert!(app.world().resource::<Selection>().contains(ea));

    // Shift-press on A, then drag a band that fully encloses B: the gesture
    // converts to an additive rubber band instead of dead-ending (and never
    // moves A).
    let before = stack_undo_len(&app);
    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ShiftLeft);
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(300.0, -40.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    let selection = app.world().resource::<Selection>();
    assert!(selection.contains(eb), "band added B");
    assert!(selection.contains(ea), "additive: A stayed selected");
    assert_eq!(stack_undo_len(&app), before, "no move was committed");
    let pos = app
        .world()
        .get::<Transform>(ea)
        .unwrap()
        .translation
        .truncate();
    assert!(pos.length() < 1e-3, "A did not move ({pos})");
}

// ---------- Weld tool: merge or make-static, never a joint (M20, 4.2) ----------

#[test]
fn weld_tool_merges_two_overlapping_bodies() {
    let mut app = paused_app();
    let a = spawn_box_at(&mut app, Vec2::ZERO, 60.0, 40.0);
    spawn_box_at(&mut app, Vec2::new(40.0, 0.0), 60.0, 40.0);
    app.update(); // colliders
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Weld);
    app.update();

    let before = stack_undo_len(&app);
    // Click in the overlap: both bodies are under the cursor.
    set_cursor(&mut app, Vec2::new(25.0, 5.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    assert_eq!(body_count(&mut app), 1, "the two bodies merged into one");
    assert_eq!(stack_undo_len(&app), before + 1, "one undoable command");
    let entity = entity_of(&app, a).expect("first target survives");
    assert!(
        matches!(
            app.world().get::<ShapeDef>(entity).unwrap(),
            ShapeDef::Csg { .. }
        ),
        "merged shape is a union tree"
    );

    undo(&mut app);
    assert_eq!(body_count(&mut app), 2, "undo splits the merge back");
}

#[test]
fn weld_tool_pins_a_single_body_by_making_it_static() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 60.0, 40.0);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Weld);
    app.update();

    let before = stack_undo_len(&app);
    set_cursor(&mut app, Vec2::new(5.0, 5.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    let entity = entity_of(&app, id).unwrap();
    assert_eq!(
        *app.world().get::<RigidBody>(entity).unwrap(),
        RigidBody::Static,
        "welding a lone body pins it to the world"
    );
    assert_eq!(stack_undo_len(&app), before + 1);

    undo(&mut app);
    let entity = entity_of(&app, id).unwrap();
    assert_eq!(
        *app.world().get::<RigidBody>(entity).unwrap(),
        RigidBody::Dynamic,
        "undo restores the dynamic body"
    );

    // Welding an already-static body is a no-op (no dead undo entry).
    let before = stack_undo_len(&app);
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id,
            old: PropertyValue::RigidBody(RigidBody::Dynamic),
            new: PropertyValue::RigidBody(RigidBody::Static),
        }],
    });
    app.update();
    set_cursor(&mut app, Vec2::new(5.0, 5.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();
    assert_eq!(
        stack_undo_len(&app),
        before + 1,
        "only the explicit edit committed; the redundant weld did not"
    );
}

// ---------- Slider default limits (M20: the drag draws the travel) ----------

#[test]
fn slider_drag_defaults_travel_limits_to_the_drag_length() {
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::ZERO, 60.0, 40.0);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Slider);
    app.update();

    // Press on the body, drag 120px along +X, release.
    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(120.0, 0.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    let mut q = app.world_mut().query::<&JointDef>();
    let def = q.iter(app.world()).next().expect("slider authored");
    let JointKind::Slider { limits, .. } = &def.kind else {
        panic!("expected a slider, got {:?}", def.kind);
    };
    let [min, max] = limits.expect("drag length becomes the default travel");
    assert!(min.abs() < 1e-3, "travel starts at the anchor ({min})");
    assert!(
        (max - 120.0).abs() < 15.0,
        "travel ends at the release ({max})"
    );
}

#[test]
fn slider_limits_default_can_be_turned_off() {
    let mut app = paused_app();
    spawn_box_at(&mut app, Vec2::ZERO, 60.0, 40.0);
    app.update();
    app.world_mut()
        .insert_resource(gradiance::domain::settings::ToolDefaults {
            slider_limits: false,
        });
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Slider);
    app.update();

    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    set_cursor(&mut app, Vec2::new(120.0, 0.0));
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();

    let mut q = app.world_mut().query::<&JointDef>();
    let def = q.iter(app.world()).next().expect("slider authored");
    assert!(
        matches!(def.kind, JointKind::Slider { limits: None, .. }),
        "option off restores unlimited sliders ({:?})",
        def.kind
    );
}

// ---------- Play-mode rotate (physical twist, feedback 2.6) ----------

#[test]
fn play_mode_right_drag_spins_physically_without_a_command() {
    let mut app = paused_app();
    let id = spawn_box_at(&mut app, Vec2::ZERO, 100.0, 100.0);
    app.update();
    let entity = entity_of(&app, id).unwrap();

    // Select it with a plain click, then enter play mode.
    set_cursor(&mut app, Vec2::ZERO);
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    app.update();

    let before = stack_undo_len(&app);
    // Right-press on the selection and swing ~90 deg about its center.
    set_cursor(&mut app, Vec2::new(30.0, 0.0));
    mouse(&mut app, MouseButton::Right, true);
    app.update();
    set_cursor(&mut app, Vec2::new(0.0, 30.0));
    step(&mut app, 3);

    let spin = app.world().get::<AngularVelocity>(entity).unwrap().0;
    assert!(spin > 0.1, "twist servo spins the body ({spin})");
    assert!(
        !app.world().resource::<MouseTwist>().0.is_empty(),
        "twist active during the drag"
    );

    mouse(&mut app, MouseButton::Right, false);
    app.update();
    assert!(
        app.world().resource::<MouseTwist>().0.is_empty(),
        "release clears the twist"
    );
    assert_eq!(
        stack_undo_len(&app),
        before,
        "physical rotate is not undoable"
    );
}

#[test]
fn play_mode_rotate_cannot_twist_through_a_prismatic() {
    let mut app = paused_app();
    let mut base = box_record(Vec2::ZERO, 40.0, 40.0);
    base.physics.rigid_body = RigidBody::Static;
    let base_id = base.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: base });
    app.update();
    let arm = spawn_box_at(&mut app, Vec2::new(120.0, 0.0), 100.0, 20.0);
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Slider {
                    axis: Vec2::X,
                    limits: Some([0.0, 100.0]),
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: base_id,
                body_b: Some(arm),
                anchor_a: Vec2::new(20.0, 0.0),
                anchor_b: Vec2::new(-50.0, 0.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();

    // Select the arm, play, and right-drag rotate hard for a while: the
    // torque-based twist must not punch through the prismatic's angular
    // constraint (feedback: "if I wanted it to rotate I'd attach a hinge").
    set_cursor(&mut app, Vec2::new(120.0, 0.0));
    mouse(&mut app, MouseButton::Left, true);
    app.update();
    mouse(&mut app, MouseButton::Left, false);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    app.update();

    set_cursor(&mut app, Vec2::new(150.0, 0.0));
    mouse(&mut app, MouseButton::Right, true);
    app.update();
    set_cursor(&mut app, Vec2::new(120.0, 90.0)); // demand ~+70 deg
    step(&mut app, 120);
    mouse(&mut app, MouseButton::Right, false);
    app.update();

    let entity = entity_of(&app, arm).unwrap();
    let rot = PosRot::from_transform(app.world().get::<Transform>(entity).unwrap()).rot;
    assert!(
        rot.abs() < 0.08,
        "the prismatic held against the rotate gesture (rot {rot})"
    );
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
