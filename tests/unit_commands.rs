use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::input::commands::{GameCommand, CommandStack, SpawnShapeCommand, SpawnJointCommand, SpawnFixedJointCommand, SpawnGroundCommand};
use gradiance::input::editable_shape::{EditableShape, ShapeType};
use gradiance::input::tools::connector::Connector;
use gradiance::input::ZIndex as GameZIndex;
use gradiance::geometry::extrusion::{ExtrudableShape, ExtrusionPlugin};
use gradiance::physics::floor::GroundPlane;
use rstest::{fixture, rstest};

#[fixture]
fn world() -> World {
    let mut world = World::new();
    world.init_resource::<GameZIndex>();
    // Initialize Assets for ExtrusionPlugin hook
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<StandardMaterial>>();
    world
}

#[rstest]
fn test_spawn_polygon_command_failure(mut world: World) {
    let vertices = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]; // Only 2 vertices
    let mut cmd = SpawnShapeCommand {
        position: Vec2::new(0.0, 0.0),
        shape: ShapeType::Polygon { points: vertices },
        entity: None,
    };

    // Apply should fail (generate_shape_components returns None)
    let result = cmd.apply(&mut world);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Invalid shape parameters");
    assert!(cmd.entity.is_none());
}

#[rstest]
fn test_spawn_box_command(mut world: World) {
    let mut cmd = SpawnShapeCommand {
        position: Vec2::new(10.0, 20.0),
        shape: ShapeType::Box {
            width: 5.0,
            height: 5.0,
        },
        entity: None,
    };

    // Apply
    assert!(cmd.apply(&mut world).is_ok());

    assert!(cmd.entity.is_some());
    let entity = cmd.entity.unwrap();

    let transform = world.get::<Transform>(entity);
    assert!(transform.is_some());
    assert_eq!(
        transform.unwrap().translation.truncate(),
        Vec2::new(10.0, 20.0)
    );

    assert!(world.get::<RigidBody>(entity).is_some());
    assert!(world.get::<Collider>(entity).is_some());
    assert!(world.get::<EditableShape>(entity).is_some());
    assert!(world.get::<ExtrudableShape>(entity).is_some());

    // Undo
    cmd.undo(&mut world);

    // In Bevy 0.15+, get_entity returns a Result. If despawned, it should be Err.
    assert!(world.get_entity(entity).is_err());
    assert!(cmd.entity.is_none());
}

#[rstest]
fn test_spawn_circle_command(mut world: World) {
    let mut cmd = SpawnShapeCommand {
        position: Vec2::new(-5.0, 5.0),
        shape: ShapeType::Circle { radius: 3.0 },
        entity: None,
    };

    // Apply
    assert!(cmd.apply(&mut world).is_ok());

    assert!(cmd.entity.is_some());
    let entity = cmd.entity.unwrap();

    let transform = world.get::<Transform>(entity);
    assert!(transform.is_some());
    assert_eq!(
        transform.unwrap().translation.truncate(),
        Vec2::new(-5.0, 5.0)
    );

    assert!(world.get::<RigidBody>(entity).is_some());
    assert!(world.get::<Collider>(entity).is_some());
    assert!(world.get::<EditableShape>(entity).is_some());

    // Undo
    cmd.undo(&mut world);

    assert!(world.get_entity(entity).is_err());
    assert!(cmd.entity.is_none());
}

#[rstest]
fn test_spawn_joint_command(mut world: World) {
    // Setup entity_a
    let entity_a = world.spawn(Transform::default()).id();

    let mut cmd = SpawnJointCommand {
        entity_a,
        entity_b: None, // Pin to world
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
    };

    // Apply
    assert!(cmd.apply(&mut world).is_ok());

    // Check ImpulseJoint on entity_a
    assert!(world.get::<ImpulseJoint>(entity_a).is_some());

    // Check visual entity spawned (child of entity_a)
    let children = world.get::<Children>(entity_a);
    assert!(children.is_some());
    let visual_id = children
        .unwrap()
        .iter()
        .find(|&&child| world.get::<Connector>(child).is_some());
    assert!(visual_id.is_some());
    let visual_id = *visual_id.unwrap();

    // Check pin entity
    assert!(cmd.pin_entity.is_some());
    let pin_id = cmd.pin_entity.unwrap();
    assert!(world.get::<RigidBody>(pin_id).is_some());

    // Undo
    cmd.undo(&mut world);

    // Check ImpulseJoint removed
    assert!(world.get::<ImpulseJoint>(entity_a).is_none());

    // Check visual entity despawned
    assert!(world.get_entity(visual_id).is_err());

    // Check pin entity despawned
    assert!(world.get_entity(pin_id).is_err());
}

#[rstest]
fn test_spawn_polygon_command(mut world: World) {
    let vertices = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(0.0, 10.0),
    ];
    let mut cmd = SpawnShapeCommand {
        position: Vec2::new(0.0, 0.0),
        shape: ShapeType::Polygon { points: vertices },
        entity: None,
    };

    // Apply
    assert!(cmd.apply(&mut world).is_ok());

    assert!(cmd.entity.is_some());
    let entity = cmd.entity.unwrap();

    assert!(world.get::<RigidBody>(entity).is_some());
    assert!(world.get::<Collider>(entity).is_some());
    assert!(world.get::<Transform>(entity).is_some());

    // Undo
    cmd.undo(&mut world);

    assert!(world.get_entity(entity).is_err());
    assert!(cmd.entity.is_none());
}

#[rstest]
fn test_command_stack(mut world: World) {
    let mut stack = CommandStack::default();

    // 1. Push Box
    let box_cmd = Box::new(SpawnShapeCommand {
        position: Vec2::ZERO,
        shape: ShapeType::Box {
            width: 1.0,
            height: 1.0,
        },
        entity: None,
    });
    stack.push(box_cmd, &mut world);

    assert_eq!(stack.current_index(), 1);
    assert_eq!(stack.history_len(), 1);
    assert_eq!(world.entities().len(), 1);

    // 2. Undo
    stack.undo(&mut world);
    assert_eq!(stack.current_index(), 0);
    assert_eq!(stack.history_len(), 1);
    assert_eq!(world.entities().len(), 0);

    // 3. Redo
    stack.redo(&mut world);
    assert_eq!(stack.current_index(), 1);
    assert_eq!(world.entities().len(), 1);

    // 4. Undo again
    stack.undo(&mut world);
    assert_eq!(stack.current_index(), 0);
    assert_eq!(world.entities().len(), 0);

    // 5. Push new command (Circle), should truncate history
    let circle_cmd = Box::new(SpawnShapeCommand {
        position: Vec2::new(10.0, 0.0),
        shape: ShapeType::Circle { radius: 1.0 },
        entity: None,
    });
    stack.push(circle_cmd, &mut world);

    assert_eq!(stack.current_index(), 1);
    assert_eq!(stack.history_len(), 1);
    assert_eq!(world.entities().len(), 1);

    let entity = world.iter_entities().next().unwrap().id();
    assert!(world.get::<EditableShape>(entity).is_some());
}

#[rstest]
fn test_spawn_fixed_joint_command(mut world: World) {
    let entity_a = world.spawn(Transform::default()).id();

    let mut cmd = SpawnFixedJointCommand {
        entity_a,
        entity_b: None,
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
        rot_a: 0.0,
        rot_b: 0.0,
    };

    // Apply
    assert!(cmd.apply(&mut world).is_ok());

    assert!(world.get::<ImpulseJoint>(entity_a).is_some());

    let children = world.get::<Children>(entity_a);
    assert!(children.is_some());
    let visual_id = *children
        .unwrap()
        .iter()
        .find(|&&child| world.get::<Connector>(child).is_some())
        .unwrap();

    assert!(cmd.pin_entity.is_some());
    let pin_id = cmd.pin_entity.unwrap();
    assert!(world.get::<RigidBody>(pin_id).is_some());

    // Undo
    cmd.undo(&mut world);

    assert!(world.get::<ImpulseJoint>(entity_a).is_none());
    assert!(world.get_entity(pin_id).is_err());
    assert!(world.get_entity(visual_id).is_err());
}

#[rstest]
fn test_spawn_joint_command_two_bodies(mut world: World) {
    let entity_a = world.spawn(Transform::default()).id();
    let entity_b = world.spawn(Transform::default()).id();

    let mut cmd = SpawnJointCommand {
        entity_a,
        entity_b: Some(entity_b),
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
    };

    // Apply
    assert!(cmd.apply(&mut world).is_ok());

    assert!(world.get::<ImpulseJoint>(entity_a).is_some());

    let joint = world.get::<ImpulseJoint>(entity_a).unwrap();
    assert_eq!(joint.parent, entity_b);

    let children = world.get::<Children>(entity_a);
    assert!(children.is_some());
    let visual_id = *children
        .unwrap()
        .iter()
        .find(|&&child| world.get::<Connector>(child).is_some())
        .unwrap();

    assert!(cmd.pin_entity.is_none());

    // Undo
    cmd.undo(&mut world);

    assert!(world.get::<ImpulseJoint>(entity_a).is_none());
    assert!(world.get_entity(visual_id).is_err());
}

#[rstest]
fn test_spawn_ground_command(mut world: World) {
    let mut cmd = SpawnGroundCommand {
        position: Vec2::new(10.0, 10.0),
        rotation: 0.0,
        entity: None,
    };

    // Apply
    assert!(cmd.apply(&mut world).is_ok());

    assert!(cmd.entity.is_some());
    let entity = cmd.entity.unwrap();

    assert!(world.get::<RigidBody>(entity).is_some());
    assert!(world.get::<Collider>(entity).is_some());
    assert!(world.get::<GroundPlane>(entity).is_some());
    assert!(world.get::<Transform>(entity).is_some());

    // Undo
    cmd.undo(&mut world);

    assert!(world.get_entity(entity).is_err());
}
