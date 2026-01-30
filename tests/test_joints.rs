use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::input::commands::{GameCommand, SpawnPrismaticJointCommand, SpawnJointCommand};
// use gradiance::prelude::*;

#[test]
fn test_spawn_prismatic_joint() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    // Setup entities
    let entity_a = app.world_mut().spawn((
        RigidBody::Dynamic,
        Transform::default(),
        GlobalTransform::default(),
    )).id();

    let entity_b = app.world_mut().spawn((
        RigidBody::Dynamic,
        Transform::from_xyz(10.0, 0.0, 0.0),
        GlobalTransform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
    )).id();

    let mut command = SpawnPrismaticJointCommand {
        entity_a,
        entity_b: Some(entity_b),
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        axis: Vec2::X,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
    };

    command.apply(app.world_mut()).expect("Failed to apply command");

    // Verify component
    let joint = app.world().get::<ImpulseJoint>(entity_a).expect("Joint not found");
    assert_eq!(joint.parent, entity_b);

    if let TypedJoint::PrismaticJoint(prism) = joint.data {
        // Rapier uses nalgebra::Vector, Bevy uses Vec2. Bevy Rapier converts them.
        // Assuming axis 1 is set correctly.
        assert!((prism.local_axis1().x - 1.0).abs() < 1e-5);
    } else {
        panic!("Joint is not Prismatic");
    }
}

#[test]
fn test_joint_motor_properties() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    let entity_a = app.world_mut().spawn((
        RigidBody::Dynamic,
        Transform::default(),
        GlobalTransform::default(),
    )).id();

    let entity_b = app.world_mut().spawn((
        RigidBody::Dynamic,
        Transform::from_xyz(10.0, 0.0, 0.0),
        GlobalTransform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
    )).id();

    // Spawn Revolute
    let mut cmd_rev = SpawnJointCommand {
        entity_a,
        entity_b: Some(entity_b),
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
    };
    cmd_rev.apply(app.world_mut()).unwrap();

    // Modify Motor
    let mut joint = app.world_mut().get_mut::<ImpulseJoint>(entity_a).unwrap();
    if let TypedJoint::RevoluteJoint(ref mut rev) = joint.data {
        rev.set_motor_velocity(5.0, 0.5); // target vel, damping
        rev.set_motor_max_force(100.0);
    }

    // Verify
    let joint = app.world().get::<ImpulseJoint>(entity_a).unwrap();
    if let TypedJoint::RevoluteJoint(rev) = joint.data {
        // Access fields via .data.raw.as_revolute()
        let raw = rev.data.raw.as_revolute().unwrap();
        let motor = &raw.data.motors[2];
        assert!(motor.target_vel == 5.0);
        assert!(motor.damping == 0.5);
        assert!(motor.max_force == 100.0);
    } else {
        panic!("Not revolute");
    }
}
