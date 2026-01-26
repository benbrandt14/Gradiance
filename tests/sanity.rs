use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

#[test]
fn test_minimal_app() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.update();
    assert!(true);
}

#[test]
fn test_revolute_stability_repro() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));
    app.add_plugins(bevy::hierarchy::HierarchyPlugin);
    app.add_plugins(TransformPlugin);

    // Initial State
    // T1 at 0,0. T2 at 0,-20, rotated 1.0 rad.
    let t1 = Transform::from_xyz(0.0, 0.0, 0.0);
    let t2 = Transform::from_xyz(0.0, -20.0, 0.0).with_rotation(Quat::from_rotation_z(1.0));

    let id1 = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            t1,
            GlobalTransform::from(t1),
            Collider::cuboid(1.0, 1.0),
            CollisionGroups::new(Group::GROUP_1, Group::NONE),
            GravityScale(0.0),
            Velocity::zero(),
            Damping {
                linear_damping: 0.0,
                angular_damping: 0.0,
            },
        ))
        .id();

    let id2 = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            t2,
            GlobalTransform::from(t2),
            Collider::cuboid(1.0, 1.0),
            CollisionGroups::new(Group::GROUP_2, Group::NONE),
            GravityScale(0.0),
            Velocity::zero(),
            Damping {
                linear_damping: 0.0,
                angular_damping: 0.0,
            },
        ))
        .id();

    app.update();

    // Calculate Anchors manually (Standard "Positive" calculation)
    let local_anchor_1 = Vec2::ZERO;

    let rot = 1.0f32;
    let cos = rot.cos();
    let sin = rot.sin();
    let rel = Vec2::new(0.0, 20.0);
    let local_anchor_2 = Vec2::new(rel.x * cos + rel.y * sin, -rel.x * sin + rel.y * cos);

    // Create Joint (Revolute) with NEGATED anchors (mimicking commands.rs fix)
    let joint_data = RevoluteJointBuilder::new()
        .local_anchor1(-local_anchor_1)
        .local_anchor2(-local_anchor_2);

    app.world_mut()
        .entity_mut(id1)
        .insert(ImpulseJoint::new(id2, joint_data));

    // Update
    for i in 0..10 {
        app.update();
        let t1_curr = app.world().get::<Transform>(id1).unwrap().translation;
        let t2_curr = app.world().get::<Transform>(id2).unwrap().translation;
        println!("Step {}: T1={:?}, T2={:?}", i, t1_curr, t2_curr);
    }

    let t1_final = app.world().get::<Transform>(id1).unwrap().translation;
    assert!((t1_final - t1.translation).length() < 10.0, "Moved!");
}
