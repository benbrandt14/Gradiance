use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::input::ToolState;
use gradiance::input::commands::CommandStack;
use gradiance::input::editable::{EditableBox, EditableCircle};
use gradiance::input::selection::Selection;
use rstest::{fixture, rstest};

mod test_utils;
use test_utils::*;

#[fixture]
fn app() -> App {
    create_test_app()
}

#[rstest]
fn test_box_tool_spawn(mut app: App) {
    set_tool(&mut app, ToolState::Box);

    // Drag from (0,0) to (10,10)
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Process command

    // Verify
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<EditableBox>, With<RigidBody>)>();
    assert_eq!(query.iter(app.world()).count(), 1);
}

#[rstest]
fn test_circle_tool_spawn(mut app: App) {
    set_tool(&mut app, ToolState::Circle);

    // Drag from (0,0) to (5,0) -> Radius 5
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, Vec2::new(5.0, 0.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Process command

    // Verify
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<EditableCircle>, With<RigidBody>)>();
    assert_eq!(query.iter(app.world()).count(), 1);
}

#[rstest]
fn test_polygon_tool_spawn(mut app: App) {
    set_tool(&mut app, ToolState::Polygon);

    // Triangle: (0,0) -> (10,0) -> (0,10) -> (0,0)

    // Point 1
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Point 2
    set_cursor(&mut app, Vec2::new(10.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Point 3
    set_cursor(&mut app, Vec2::new(0.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Close loop (click near start)
    set_cursor(&mut app, Vec2::new(0.1, 0.1));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Process command

    // Verify
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<RigidBody>, With<Collider>)>();
    // Note: We expect 1 entity.
    assert_eq!(query.iter(app.world()).count(), 1);
}

#[rstest]
fn test_selection_tool(mut app: App) {
    // 1. Spawn a box manually
    let box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(10.0, 10.0, 0.0),
            GlobalTransform::default(), // Important for spatial query
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
        ))
        .id();

    // Run physics update to populate spatial index
    for _ in 0..5 {
        app.update();
    }

    set_tool(&mut app, ToolState::Select);

    // 2. Click on the box
    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify selection
    let selection = app.world().resource::<Selection>();
    assert!(selection.0.contains(&box_entity), "Box should be selected");

    // 3. Click elsewhere
    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify cleared
    let selection = app.world().resource::<Selection>();
    assert!(selection.0.is_empty(), "Selection should be cleared");
}

#[rstest]
fn test_joint_tool_pin(mut app: App) {
    // 1. Spawn a box
    let box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(5.0, 5.0, 0.0),
            GlobalTransform::default(),
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
        ))
        .id();

    // Update physics
    for _ in 0..5 {
        app.update();
    }

    set_tool(&mut app, ToolState::RevoluteJoint);

    // 2. Click on the box to pin it
    set_cursor(&mut app, Vec2::new(5.0, 5.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    let has_joint = app.world().get::<ImpulseJoint>(box_entity).is_some();

    assert!(
        has_joint,
        "Should have created an ImpulseJoint on the entity"
    );
}

#[rstest]
fn test_drag_tool(mut app: App) {
    // 1. Spawn a dynamic box
    let _box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(5.0, 5.0, 0.0),
            GlobalTransform::default(),
            Velocity::default(),
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
        ))
        .id();

    // Update physics
    for _ in 0..5 {
        app.update();
    }

    set_tool(&mut app, ToolState::Drag);

    // 2. Click on the box
    set_cursor(&mut app, Vec2::new(5.0, 5.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update(); // Trigger spawn of hand entity

    // Verify hand entity exists
    let mut hand_query = app
        .world_mut()
        .query_filtered::<Entity, (With<RigidBody>, Without<Collider>)>();
    // There should be one hand entity (KinematicPositionBased)
    let hands: Vec<Entity> = hand_query.iter(app.world()).collect();
    // Filter to ensure it's not the box (Box has Collider)
    assert_eq!(hands.len(), 1, "Should have spawned one hand entity");

    // 3. Move cursor
    let target_pos = Vec2::new(10.0, 10.0);
    set_cursor(&mut app, target_pos);
    app.update(); // Hand should move

    // Verify hand moved
    let hand_entity = hands[0];
    let hand_transform = app.world().get::<Transform>(hand_entity).unwrap();
    assert_eq!(
        hand_transform.translation.truncate(),
        target_pos,
        "Hand should follow cursor"
    );

    // 4. Release mouse
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify hand is despawned
    assert!(
        app.world().get_entity(hand_entity).is_err(),
        "Hand should be despawned"
    );
}

#[rstest]
fn test_undo_redo(mut app: App) {
    set_tool(&mut app, ToolState::Box);

    // 1. Spawn a box
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Process command

    // Verify box exists
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<EditableBox>, With<RigidBody>)>();
    let entities: Vec<Entity> = query.iter(app.world()).collect();
    assert_eq!(entities.len(), 1, "Box should be spawned");
    let _box_id = entities[0];

    // 2. Undo
    // CommandStack requires mutable access to world to undo
    app.world_mut()
        .resource_scope(|world, mut stack: Mut<CommandStack>| {
            stack.undo(world);
        });
    app.update(); // Process any despawns

    // Verify box is gone
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<EditableBox>, With<RigidBody>)>();
    assert_eq!(
        query.iter(app.world()).count(),
        0,
        "Box should be removed after undo"
    );

    // 3. Redo
    app.world_mut()
        .resource_scope(|world, mut stack: Mut<CommandStack>| {
            stack.redo(world);
        });
    app.update();

    // Verify box is back
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<EditableBox>, With<RigidBody>)>();
    assert_eq!(
        query.iter(app.world()).count(),
        1,
        "Box should be restored after redo"
    );
}
