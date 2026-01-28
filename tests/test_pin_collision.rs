use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::input::commands::{GameCommand, SpawnJointCommand};
use rstest::{fixture, rstest};

mod test_utils;
use test_utils::*;

const PIN_GROUP: Group = Group::GROUP_32;

#[fixture]
fn app() -> App {
    create_test_app()
}

#[rstest]
fn test_pin_collision_solver_groups(mut app: App) {
    // 1. Spawn a box
    let box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::default(),
            GlobalTransform::default(),
        ))
        .id();

    // Verify initial SolverGroups (None)
    assert!(app.world().get::<SolverGroups>(box_entity).is_none());

    // 2. Apply SpawnJointCommand
    let mut cmd = SpawnJointCommand {
        entity_a: box_entity,
        entity_b: None, // Pin to world
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
    };

    cmd.apply(app.world_mut()).unwrap();
    app.update();

    // 3. Verify Pin Entity
    let pin_entity = cmd.pin_entity.expect("Pin entity should be spawned");

    // Check Collider
    assert!(
        app.world().get::<Collider>(pin_entity).is_some(),
        "Pin should have a collider"
    );

    // Check Pin SolverGroups
    let pin_groups = app
        .world()
        .get::<SolverGroups>(pin_entity)
        .expect("Pin should have SolverGroups");
    assert_eq!(pin_groups.memberships, PIN_GROUP);
    assert_eq!(pin_groups.filters, Group::ALL);

    // 4. Verify Box Entity
    let box_groups = app
        .world()
        .get::<SolverGroups>(box_entity)
        .expect("Box should have SolverGroups now");
    assert_eq!(box_groups.memberships, Group::ALL);
    assert_eq!(box_groups.filters, Group::ALL & !PIN_GROUP);

    // 5. Undo
    cmd.undo(app.world_mut());
    app.update();

    // 6. Verify Box SolverGroups restored
    assert!(
        app.world().get::<SolverGroups>(box_entity).is_none(),
        "Box SolverGroups should be removed"
    );

    // Verify Pin is gone
    assert!(app.world().get_entity(pin_entity).is_err());
}

#[rstest]
fn test_joint_two_bodies_no_solver_group_change(mut app: App) {
    let entity_a = app.world_mut().spawn(Transform::default()).id();
    let entity_b = app.world_mut().spawn(Transform::default()).id();

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

    cmd.apply(app.world_mut()).unwrap();
    app.update();

    // Check SolverGroups on entity_a
    assert!(
        app.world().get::<SolverGroups>(entity_a).is_none(),
        "Should NOT modify SolverGroups for 2-body joint"
    );

    // Check Undo
    cmd.undo(app.world_mut());
    assert!(app.world().get::<SolverGroups>(entity_a).is_none());
}
