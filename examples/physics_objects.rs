use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, setup_physics)
        .run();
}

fn setup_physics(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Ground
    commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(500.0, 50.0),
        Transform::from_xyz(0.0, -300.0, 0.0),
    ));

    // 1. Different Shapes

    // Box
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(30.0, 30.0),
        Transform::from_xyz(-300.0, 200.0, 0.0),
        Restitution::coefficient(0.7),
    ));

    // Ball
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(30.0),
        Transform::from_xyz(-200.0, 200.0, 0.0),
        Restitution::coefficient(0.7),
    ));

    // Capsule
    commands.spawn((
        RigidBody::Dynamic,
        Collider::capsule_y(20.0, 10.0),
        Transform::from_xyz(-100.0, 200.0, 0.0),
    ));

    // Polygon (Triangle)
    let vertices = vec![
        Vec2::new(-20.0, -20.0),
        Vec2::new(20.0, -20.0),
        Vec2::new(0.0, 20.0),
    ];
    commands.spawn((
        RigidBody::Dynamic,
        Collider::convex_hull(&vertices).unwrap(),
        Transform::from_xyz(0.0, 200.0, 0.0),
    ));

    // Compound Shape (multiple colliders on children)
    let parent = commands.spawn((
        RigidBody::Dynamic,
        Transform::from_xyz(100.0, 200.0, 0.0),
        // Add a dummy collider or sleep disabled to ensure it participates?
        // Actually, without a collider on the parent, Rapier needs children with colliders.
        // Bevy Rapier handles this by looking for children with colliders.
    )).id();

    commands.spawn((
        Collider::cuboid(20.0, 10.0),
        Transform::from_xyz(0.0, 10.0, 0.0),
    )).set_parent(parent);

    commands.spawn((
        Collider::cuboid(20.0, 10.0),
        Transform::from_xyz(0.0, -10.0, 0.0),
        Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
    )).set_parent(parent);

    // 2. Joints

    // Revolute Joint (Hinge)
    let anchor = commands.spawn((
        RigidBody::Fixed,
        Collider::ball(5.0),
        Transform::from_xyz(200.0, 100.0, 0.0),
    )).id();

    let dangling_box = commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(10.0, 50.0),
        Transform::from_xyz(200.0, 50.0, 0.0),
    )).id();

    let joint = RevoluteJointBuilder::new()
        .local_anchor1(Vec2::new(0.0, 0.0))
        .local_anchor2(Vec2::new(0.0, 50.0));

    commands.entity(dangling_box).insert(ImpulseJoint::new(anchor, joint));

    // Prismatic Joint (Slider)
    let slider_base = commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(50.0, 10.0),
        Transform::from_xyz(350.0, 100.0, 0.0),
    )).id();

    let slider_part = commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(20.0, 20.0),
        Transform::from_xyz(350.0, 150.0, 0.0),
    )).id();

    let prismatic = PrismaticJointBuilder::new(Vec2::Y)
        .local_anchor1(Vec2::new(0.0, 20.0))
        .local_anchor2(Vec2::new(0.0, -30.0))
        .limits([-50.0, 50.0]);

    commands.entity(slider_part).insert(ImpulseJoint::new(slider_base, prismatic));

    // Instructions
    commands.spawn((
        Text::new("Shapes: Box, Ball, Capsule, Triangle, Compound (+)\nJoints: Revolute (Hinge), Prismatic (Slider)"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}
