use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::input::commands::{GameCommand, SpawnJointCommand, SpawnPrismaticJointCommand};
use gradiance::prelude::EntityId;

#[test]
fn test_spawn_prismatic_joint() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    // Setup entities
    let entity_a = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Transform::default(),
            GlobalTransform::default(),
        ))
        .id();

    let entity_b = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Transform::from_xyz(10.0, 0.0, 0.0),
            GlobalTransform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        ))
        .id();

    let mut command = SpawnPrismaticJointCommand {
        entity_a: EntityId(entity_a),
        entity_b: Some(EntityId(entity_b)),
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        axis: Vec2::X,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
        original_collision_groups: None,
    };

    command
        .apply(app.world_mut())
        .expect("Failed to apply command");

    // Verify component
    let joint = app
        .world()
        .get::<ImpulseJoint>(entity_a)
        .expect("Joint not found");
    assert_eq!(joint.parent, entity_b);

    match joint.data {
        TypedJoint::PrismaticJoint(prism) => {
            assert!((prism.local_axis1().x - 1.0).abs() < 1e-5);
        }
        TypedJoint::GenericJoint(generic) => {
            // Prismatic allows X movement.
            // Bevy Rapier GenericJoint wrapping is opaque, but we can check constraints.
            // Actually, we can check if it supports X axis.
            use bevy_rapier2d::rapier::dynamics::JointAxesMask;
            assert!(generic.locked_axes().contains(JointAxesMask::LIN_Y));
            assert!(generic.locked_axes().contains(JointAxesMask::ANG_X));
            assert!(!generic.locked_axes().contains(JointAxesMask::LIN_X));
        }
        _ => panic!("Joint is not Prismatic or Generic Prismatic"),
    }
}

#[test]
fn test_joint_motor_properties() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    let entity_a = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Transform::default(),
            GlobalTransform::default(),
        ))
        .id();

    let entity_b = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Transform::from_xyz(10.0, 0.0, 0.0),
            GlobalTransform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        ))
        .id();

    // Spawn Revolute
    let mut cmd_rev = SpawnJointCommand {
        entity_a: EntityId(entity_a),
        entity_b: Some(EntityId(entity_b)),
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
        compliance: 0.0,
        visual_entity: None,
        pin_entity: None,
        original_solver_groups: None,
        original_collision_groups: None,
    };
    cmd_rev.apply(app.world_mut()).unwrap();

    // Modify Motor
    use bevy_rapier2d::rapier::dynamics::JointAxis;
    let mut joint = app.world_mut().get_mut::<ImpulseJoint>(entity_a).unwrap();
    match &mut joint.data {
        TypedJoint::RevoluteJoint(rev) => {
            rev.set_motor_velocity(5.0, 0.5);
            rev.set_motor_max_force(100.0);
        }
        TypedJoint::GenericJoint(g) => {
            g.set_motor_velocity(JointAxis::AngX, 5.0, 0.5);
            g.set_motor_max_force(JointAxis::AngX, 100.0);
        }
        _ => panic!("Unexpected joint type for update"),
    }

    // Verify
    let joint = app.world().get::<ImpulseJoint>(entity_a).unwrap();
    match &joint.data {
        TypedJoint::RevoluteJoint(rev) => {
            let raw = rev.data.raw.as_revolute().unwrap();
            let motor = &raw.data.motors[2];
            assert!(motor.target_vel == 5.0);
            assert!(motor.damping == 0.5);
            assert!(motor.max_force == 100.0);
        }
        TypedJoint::GenericJoint(g) => {
            let motor = g.motor(JointAxis::AngX).unwrap();
            assert_eq!(motor.target_vel, 5.0);
            assert_eq!(motor.damping, 0.5);
            assert_eq!(motor.max_force, 100.0);
        }
        _ => panic!("Unexpected joint type for verification"),
    }
}
