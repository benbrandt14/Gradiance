//! Interaction-layer tests: shortcuts drive intents, snapping picks the
//! right source, selection stays consistent.

use crate::harness::{body_count, box_record, entity_of, paused_app, step};
use bevy::prelude::*;
use gradiance::prelude::*;

/// Presses (and holds) keys for one frame.
fn press(app: &mut App, keys: &[KeyCode]) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    for k in keys {
        input.press(*k);
    }
    app.update();
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    for k in keys {
        input.release(*k);
    }
    app.update();
}

fn spawn_box(app: &mut App, pos: Vec2) -> StableId {
    let record = box_record(pos, 40.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

#[test]
fn ctrl_z_undoes_and_ctrl_shift_z_redoes() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO);
    assert_eq!(body_count(&mut app), 1);

    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyZ]);
    assert_eq!(body_count(&mut app), 0, "Ctrl+Z undid the spawn");

    press(
        &mut app,
        &[KeyCode::ControlLeft, KeyCode::ShiftLeft, KeyCode::KeyZ],
    );
    assert_eq!(body_count(&mut app), 1, "Ctrl+Shift+Z redid the spawn");
    assert!(entity_of(&app, id).is_some());
}

#[test]
fn delete_key_deletes_selection_undoably() {
    let mut app = paused_app();
    let id = spawn_box(&mut app, Vec2::ZERO);
    let entity = entity_of(&app, id).unwrap();
    app.world_mut().resource_mut::<Selection>().set(entity);

    press(&mut app, &[KeyCode::Delete]);
    assert_eq!(body_count(&mut app), 0);
    assert!(
        app.world().resource::<Selection>().is_empty(),
        "selection pruned after despawn"
    );

    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyZ]);
    assert_eq!(body_count(&mut app), 1, "delete is one undo step");
}

#[test]
fn space_toggles_pause_and_tool_keys_switch_tools() {
    let mut app = paused_app();
    assert_eq!(
        *app.world().resource::<State<GameState>>().get(),
        GameState::Paused
    );
    press(&mut app, &[KeyCode::Space]);
    assert_eq!(
        *app.world().resource::<State<GameState>>().get(),
        GameState::Playing
    );

    assert_eq!(
        *app.world().resource::<State<ToolState>>().get(),
        ToolState::Select
    );
    press(&mut app, &[KeyCode::KeyB]);
    assert_eq!(
        *app.world().resource::<State<ToolState>>().get(),
        ToolState::Box
    );
    press(&mut app, &[KeyCode::KeyG]);
    assert_eq!(
        *app.world().resource::<State<ToolState>>().get(),
        ToolState::Ground
    );
}

#[test]
fn ctrl_a_selects_all_and_escape_clears() {
    let mut app = paused_app();
    spawn_box(&mut app, Vec2::ZERO);
    spawn_box(&mut app, Vec2::new(100.0, 0.0));

    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyA]);
    assert_eq!(app.world().resource::<Selection>().len(), 2);

    press(&mut app, &[KeyCode::Escape]);
    assert!(app.world().resource::<Selection>().is_empty());
}

#[test]
fn select_all_and_clear_maintain_the_joint_xor_bodies_invariant() {
    use gradiance::interaction::selection::SelectedJoint;
    let mut app = paused_app();
    spawn_box(&mut app, Vec2::ZERO);
    spawn_box(&mut app, Vec2::new(100.0, 0.0));

    // Simulate a lingering joint selection (as if a joint were picked).
    // The Joint marker keeps it alive through prune_dead_selection, so
    // only the transition can clear it — isolating the invariant.
    let phantom = app.world_mut().spawn(gradiance::domain::Joint).id();
    app.world_mut().resource_mut::<SelectedJoint>().0 = Some(phantom);

    // Ctrl+A must select the bodies AND drop the joint — the two are
    // never populated at once.
    press(&mut app, &[KeyCode::ControlLeft, KeyCode::KeyA]);
    assert_eq!(app.world().resource::<Selection>().len(), 2);
    assert!(
        app.world().resource::<SelectedJoint>().0.is_none(),
        "selecting bodies clears the joint (invariant)"
    );

    // Re-arm the joint, then Escape clears everything.
    app.world_mut().resource_mut::<SelectedJoint>().0 = Some(phantom);
    press(&mut app, &[KeyCode::Escape]);
    assert!(app.world().resource::<Selection>().is_empty());
    assert!(
        app.world().resource::<SelectedJoint>().0.is_none(),
        "Escape clears both bodies and joint"
    );
}

#[test]
fn object_snap_beats_grid_and_picks_the_vertex() {
    let mut app = paused_app();
    // Box centered at origin: corners at (±20, ±10).
    spawn_box(&mut app, Vec2::ZERO);
    {
        let mut grid = app.world_mut().resource_mut::<GridSettings>();
        grid.snap_enabled = true;
    }
    step(&mut app, 2); // let colliders build

    // Cursor near the (20, 10) corner (also near grid point (0,0)? no —
    // near (0,0) lattice at 100 spacing the nearest grid point is (0,0),
    // 24px away; the vertex is 4px away and must win).
    app.world_mut()
        .insert_resource(CursorWorldPos(Some(Vec2::new(22.0, 13.0))));
    // Run only the snap computation via a manual system run.
    let _ = app
        .world_mut()
        .run_system_cached(gradiance::interaction::snap::update_snapped_cursor);

    let snapped = app.world().resource::<SnappedCursor>();
    assert_eq!(snapped.kind, Some(SnapKind::Vertex), "{snapped:?}");
    let pos = snapped.position.unwrap();
    assert!((pos - Vec2::new(20.0, 10.0)).length() < 1e-3, "{pos}");
}

#[test]
fn grid_snap_applies_when_no_object_nearby() {
    let mut app = paused_app();
    {
        let mut grid = app.world_mut().resource_mut::<GridSettings>();
        grid.snap_enabled = true;
        grid.spacing = 100.0;
    }
    app.world_mut()
        .insert_resource(CursorWorldPos(Some(Vec2::new(430.0, 690.0))));
    let _ = app
        .world_mut()
        .run_system_cached(gradiance::interaction::snap::update_snapped_cursor);

    let snapped = app.world().resource::<SnappedCursor>();
    assert_eq!(snapped.kind, Some(SnapKind::Grid));
    assert_eq!(snapped.position, Some(Vec2::new(400.0, 700.0)));
}

#[test]
fn isometric_grid_snaps_to_rhombic_lattice() {
    let mut app = paused_app();
    {
        let mut grid = app.world_mut().resource_mut::<GridSettings>();
        grid.snap_enabled = true;
        grid.system = GridSystem::Isometric;
        grid.spacing = 100.0;
    }
    app.world_mut()
        .insert_resource(CursorWorldPos(Some(Vec2::new(90.0, 45.0))));
    let _ = app
        .world_mut()
        .run_system_cached(gradiance::interaction::snap::update_snapped_cursor);

    let snapped = app.world().resource::<SnappedCursor>().position.unwrap();
    // Verify lattice residency in the iso basis.
    let a = Vec2::from_angle(30_f32.to_radians()) * 100.0;
    let b = Vec2::from_angle(150_f32.to_radians()) * 100.0;
    let det = a.perp_dot(b);
    let u = snapped.perp_dot(b) / det;
    let v = a.perp_dot(snapped) / det;
    assert!((u - u.round()).abs() < 1e-3 && (v - v.round()).abs() < 1e-3);
}

#[test]
fn gesture_constraints_follow_modifier_keys() {
    let mut app = paused_app();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyX);
    app.update();
    assert_eq!(
        app.world().resource::<GestureConstraints>().axis,
        AxisConstraint::Horizontal
    );
    let constrained = app
        .world()
        .resource::<GestureConstraints>()
        .constrain(Vec2::new(5.0, 9.0));
    assert_eq!(constrained, Vec2::new(5.0, 0.0));

    // X+Y together = dominant-axis lock; Shift is a selection modifier and
    // must NOT constrain gestures (it used to inject axis-warped offsets —
    // feedback 1.1, 2.2).
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.press(KeyCode::KeyY);
    app.update();
    assert_eq!(
        app.world().resource::<GestureConstraints>().axis,
        AxisConstraint::Dominant
    );
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.release(KeyCode::KeyX);
    input.release(KeyCode::KeyY);
    input.press(KeyCode::ShiftLeft);
    app.update();
    assert_eq!(
        app.world().resource::<GestureConstraints>().axis,
        AxisConstraint::Free,
        "shift is not an axis modifier"
    );

    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.release(KeyCode::ShiftLeft);
    input.press(KeyCode::ControlLeft);
    app.update();
    let constraints = *app.world().resource::<GestureConstraints>();
    assert!(constraints.quantize_rotation);
    let config = SnapConfig::default();
    let q = constraints.apply_rotation(17_f32.to_radians(), &config);
    assert!((q - 15_f32.to_radians()).abs() < 1e-5);
}
