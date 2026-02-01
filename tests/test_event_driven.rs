#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use gradiance::events::*;
    use gradiance::input::commands::CommandStack;
    use gradiance::input::event_handlers;
    use gradiance::input::editable_shape::ShapeType;

    #[test]
    fn test_spawn_shape_event() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(GameEventsPlugin);
        app.init_resource::<CommandStack>();
        app.add_systems(Update, event_handlers::handle_spawn_shape_event);

        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<Image>();

        // Fire event
        app.world_mut().send_event(SpawnShapeEvent {
            position: Vec2::new(10.0, 20.0),
            shape: ShapeType::Box { width: 1.0, height: 1.0 },
        });

        app.update();
        app.update();

        let mut query = app.world_mut().query::<&Transform>();
        let count = query.iter(app.world()).count();
        assert_eq!(count, 1);

        let mut query = app.world_mut().query::<&Transform>();
        let t = query.single(app.world());
        assert_eq!(t.translation.x, 10.0);
        assert_eq!(t.translation.y, 20.0);
    }

    #[test]
    fn test_move_entities_event() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(GameEventsPlugin);
        app.add_systems(Update, event_handlers::handle_drag_entities_event);

        let entity = app.world_mut().spawn(Transform::from_xyz(0.0, 0.0, 0.0)).id();

        app.world_mut().send_event(DragEntitiesEvent {
            positions: vec![(entity, Vec2::new(5.0, 5.0))],
            rotations: vec![],
        });

        app.update();

        let t = app.world().get::<Transform>(entity).unwrap();
        assert_eq!(t.translation.x, 5.0);
        assert_eq!(t.translation.y, 5.0);
    }

    #[test]
    fn test_undo_redo_movement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(GameEventsPlugin);
        app.init_resource::<CommandStack>();
        app.add_systems(Update, (
            event_handlers::handle_commit_drag_event,
            event_handlers::handle_undo_event,
            event_handlers::handle_redo_event,
        ));

        let entity = app.world_mut().spawn(Transform::from_xyz(0.0, 0.0, 0.0)).id();

        // Commit drag
        app.world_mut().send_event(CommitDragEvent {
            position_changes: vec![(entity, Vec2::ZERO, Vec2::new(10.0, 0.0))],
            rotation_changes: vec![],
        });

        app.update(); // Process event -> Push Command -> Apply

        let t = app.world().get::<Transform>(entity).unwrap();
        assert_eq!(t.translation.x, 10.0);

        // Undo
        app.world_mut().send_event(UndoEvent);
        app.update();

        let t = app.world().get::<Transform>(entity).unwrap();
        assert_eq!(t.translation.x, 0.0); // Should be back to 0

        // Redo
        app.world_mut().send_event(RedoEvent);
        app.update();

        let t = app.world().get::<Transform>(entity).unwrap();
        assert_eq!(t.translation.x, 10.0); // Should be back to 10
    }
}
