//! Command-layer integration tests: intents → dispatch → stack semantics.

use crate::harness::{body_count, box_record, entity_of, paused_app, redo, undo};
use avian2d::prelude::RigidBody;
use bevy::prelude::*;
use gradiance::command::CommandStack;
use gradiance::prelude::*;

fn stack_lens(app: &App) -> (usize, usize) {
    let stack = app.world().resource::<CommandStack>();
    (stack.undo_len(), stack.redo_len())
}

#[test]
fn spawn_intent_creates_a_complete_body() {
    let mut app = paused_app();
    let record = box_record(Vec2::new(10.0, 20.0), 40.0, 30.0);
    let id = record.id;

    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    assert_eq!(body_count(&mut app), 1);
    let entity = entity_of(&app, id).expect("body indexed by stable id");
    let world = app.world();
    assert!(world.get::<ShapeDef>(entity).is_some());
    assert!(world.get::<RigidBody>(entity).is_some());
    assert!(world.get::<Appearance>(entity).is_some());
    assert!(world.get::<DepthBand>(entity).is_some());
    let transform = world.get::<Transform>(entity).unwrap();
    assert_eq!(transform.translation.truncate(), Vec2::new(10.0, 20.0));
    assert_eq!(stack_lens(&app), (1, 0));
}

#[test]
fn invalid_shapes_are_refused_and_not_recorded() {
    let mut app = paused_app();
    let mut record = box_record(Vec2::ZERO, 40.0, 30.0);
    record.shape = ShapeDef::Box {
        width: 0.0,
        height: 30.0,
    };

    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    assert_eq!(body_count(&mut app), 0);
    assert_eq!(stack_lens(&app), (0, 0));

    let mut record = box_record(Vec2::ZERO, 1.0, 1.0);
    record.shape = ShapeDef::Polygon {
        outline: vec![Vec2::ZERO, Vec2::X],
        holes: vec![],
    };
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    assert_eq!(body_count(&mut app), 0);
    assert_eq!(stack_lens(&app), (0, 0));
}

#[test]
fn undo_redo_round_trips_a_spawn_preserving_the_id() {
    let mut app = paused_app();
    let record = box_record(Vec2::new(5.0, 5.0), 10.0, 10.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    undo(&mut app);
    assert_eq!(body_count(&mut app), 0);
    assert_eq!(entity_of(&app, id), None);
    assert_eq!(stack_lens(&app), (0, 1));

    redo(&mut app);
    assert_eq!(body_count(&mut app), 1);
    assert!(entity_of(&app, id).is_some(), "redo restores the same id");
    assert_eq!(stack_lens(&app), (1, 0));
}

#[test]
fn a_new_command_truncates_the_redo_branch() {
    let mut app = paused_app();
    app.world_mut().write_message(SpawnBodyIntent {
        record: box_record(Vec2::ZERO, 10.0, 10.0),
    });
    app.update();
    app.world_mut().write_message(SpawnBodyIntent {
        record: box_record(Vec2::new(50.0, 0.0), 10.0, 10.0),
    });
    app.update();
    assert_eq!(stack_lens(&app), (2, 0));

    undo(&mut app);
    assert_eq!(stack_lens(&app), (1, 1));

    app.world_mut().write_message(SpawnBodyIntent {
        record: box_record(Vec2::new(0.0, 50.0), 10.0, 10.0),
    });
    app.update();
    assert_eq!(stack_lens(&app), (2, 0), "redo branch dropped");
    assert_eq!(body_count(&mut app), 2);
}

#[test]
fn delete_restores_full_state_on_undo() {
    let mut app = paused_app();
    let record = box_record(Vec2::new(-3.0, 7.0), 12.0, 8.0);
    let id = record.id;
    let expected_pose = record.pose;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    app.world_mut()
        .write_message(DeleteIntent { targets: vec![id] });
    app.update();
    assert_eq!(body_count(&mut app), 0);
    assert_eq!(entity_of(&app, id), None);

    undo(&mut app);
    assert_eq!(body_count(&mut app), 1);
    let entity = entity_of(&app, id).expect("same stable id restored");
    let transform = app.world().get::<Transform>(entity).unwrap();
    assert_eq!(transform.translation.truncate(), expected_pose.pos);
}

#[test]
fn duplicate_clones_with_offset_and_reuses_ids_on_redo() {
    let mut app = paused_app();
    let record = box_record(Vec2::new(1.0, 2.0), 10.0, 10.0);
    let source = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    app.world_mut().write_message(DuplicateIntent {
        sources: vec![source],
        offset: Vec2::new(100.0, 0.0),
    });
    app.update();
    assert_eq!(body_count(&mut app), 2);

    // Find the clone: the body that isn't the source.
    let clone_id = {
        let mut ids = Vec::new();
        let mut query = app.world_mut().query_filtered::<&StableId, With<Body>>();
        for id in query.iter(app.world()) {
            if *id != source {
                ids.push(*id);
            }
        }
        assert_eq!(ids.len(), 1);
        ids[0]
    };
    let clone_entity = entity_of(&app, clone_id).unwrap();
    let clone_pos = app
        .world()
        .get::<Transform>(clone_entity)
        .unwrap()
        .translation
        .truncate();
    assert_eq!(clone_pos, Vec2::new(101.0, 2.0));

    undo(&mut app);
    assert_eq!(body_count(&mut app), 1);
    assert_eq!(entity_of(&app, clone_id), None);

    redo(&mut app);
    assert_eq!(body_count(&mut app), 2);
    assert!(
        entity_of(&app, clone_id).is_some(),
        "redo respawns the clone under the same stable id"
    );
}

#[test]
fn transform_commit_moves_and_undo_restores() {
    let mut app = paused_app();
    let record = box_record(Vec2::ZERO, 10.0, 10.0);
    let id = record.id;
    let old = record.pose;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    let new = PosRot {
        pos: Vec2::new(30.0, -10.0),
        rot: 1.0,
    };
    app.world_mut().write_message(CommitTransformIntent {
        changes: vec![TransformChange { id, old, new }],
    });
    app.update();

    let entity = entity_of(&app, id).unwrap();
    let t = app.world().get::<Transform>(entity).unwrap();
    assert_eq!(t.translation.truncate(), new.pos);

    undo(&mut app);
    // Snapshot-undo respawns the body (same StableId, fresh Entity), so
    // re-resolve rather than reuse the pre-undo handle.
    let entity = entity_of(&app, id).expect("same stable id after undo");
    let t = app.world().get::<Transform>(entity).unwrap();
    assert_eq!(t.translation.truncate(), old.pos);
    assert!(t.rotation.angle_between(Quat::IDENTITY) < 1e-5);
}

#[test]
fn id_index_tracks_spawn_and_despawn() {
    let mut app = paused_app();
    let record = box_record(Vec2::ZERO, 10.0, 10.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    assert_eq!(app.world().resource::<IdIndex>().len(), 1);

    app.world_mut()
        .write_message(DeleteIntent { targets: vec![id] });
    app.update();
    assert!(app.world().resource::<IdIndex>().is_empty());
}
