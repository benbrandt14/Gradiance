use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::physics::attraction::{Attractor, AttractionPlugin};

#[test]
fn test_attraction_force() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(TransformPlugin);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));
    app.add_plugins(AttractionPlugin);

    // Spawn attractor
    let attractor = app.world_mut().spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        GlobalTransform::default(),
        Attractor {
            strength: 1000.0,
            range: 100.0,
        },
    )).id();

    // Spawn target (Dynamic body with mass)
    let target = app.world_mut().spawn((
        Transform::from_xyz(10.0, 0.0, 0.0),
        GlobalTransform::default(),
        RigidBody::Dynamic,
        Collider::ball(1.0),
        ExternalForce::default(),
        AdditionalMassProperties::Mass(1.0),
        GravityScale(0.0),
    )).id();

    // Run updates
    // We need to run enough updates to trigger FixedUpdate where our system lives.
    // By default FixedUpdate runs at 64Hz.
    for _ in 0..100 {
        app.update();
    }

    let force = app.world().get::<ExternalForce>(target).unwrap();
    let has_mass = app.world().get::<ReadMassProperties>(target).is_some();
    let transform = app.world().get::<GlobalTransform>(target).unwrap();
    let att_transform = app.world().get::<GlobalTransform>(attractor).unwrap();
    println!("Force: {:?}, Has Mass: {}, Target Pos: {}, Attractor Pos: {}", force.force, has_mass, transform.translation(), att_transform.translation());

    // Force should be pointing towards attractor (-X direction)
    // F = 1000 * 1.0 / (10^2 + 100) = 1000 / 200 = 5.0
    assert!(force.force.x < -4.0 && force.force.x > -6.0, "Force X should be approx -5.0, got {}", force.force.x);
    assert!(force.force.y.abs() < 0.001, "Force Y should be 0, got {}", force.force.y);
}

#[test]
fn test_attraction_range() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(TransformPlugin);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));
    app.add_plugins(AttractionPlugin);

    // Spawn attractor with small range
    app.world_mut().spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        GlobalTransform::default(),
        Attractor {
            strength: 1000.0,
            range: 5.0, // Target is at 10.0
        },
    ));

    // Spawn target
    let target = app.world_mut().spawn((
        Transform::from_xyz(10.0, 0.0, 0.0),
        GlobalTransform::default(),
        RigidBody::Dynamic,
        Collider::ball(1.0),
        ExternalForce::default(),
        GravityScale(0.0),
    )).id();

    for _ in 0..100 {
        app.update();
    }

    let force = app.world().get::<ExternalForce>(target).unwrap();
    assert_eq!(force.force, Vec2::ZERO, "Force should be zero when out of range");
}
