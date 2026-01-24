use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::prelude::*;
use gradiance::geometry::gear::GearProfile;
use gradiance::input::commands::{GameCommand, SpawnGearCommand};
use gradiance::physics::constraints::{GearJoint, GearPhysicsSettings, ConstraintsPlugin};
use rstest::*;

#[fixture]
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(bevy::asset::AssetPlugin::default());
    app.add_plugins(bevy::state::app::StatesPlugin); // Needed for in_state? No, MinimalPlugins doesn't have it? Wait, ToolState usage needs StatesPlugin.
    // Actually GamePlugin adds InputPlugin which inits state.
    // But here I'm adding plugins manually.

    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));
    app.add_plugins(ConstraintsPlugin);
    app.init_resource::<gradiance::input::ZIndex>();

    // Add time plugin and setup fixed time
    app.insert_resource(Time::<Fixed>::from_seconds(1.0 / 60.0));

    app
}

#[rstest]
fn test_gear_profile_generation() {
    let profile = GearProfile::default();
    let points = profile.generate_points(5);
    assert!(!points.is_empty());
}

#[rstest]
fn test_gear_spawn_command(mut app: App) {
    let mut world = app.world_mut();

    let mut cmd = SpawnGearCommand {
        position: Vec2::ZERO,
        profile: GearProfile::default(),
        entity: None,
        pin_entity: None,
    };

    cmd.apply(world).unwrap();

    let entity = cmd.entity.unwrap();
    assert!(world.get::<Collider>(entity).is_some());
    assert!(world.get::<RigidBody>(entity).is_some());
    assert!(world.get::<ImpulseJoint>(entity).is_some());
}

#[rstest]
fn test_gear_constraint_solver(mut app: App) {
    app.world_mut().resource_mut::<GearPhysicsSettings>().analytic_enabled = true;

    let entity_a = app.world_mut().spawn((
        RigidBody::Dynamic,
        Velocity::default(),
        Collider::ball(1.0),
        // Mass properties will be calculated by Rapier
    )).id();

    let entity_b = app.world_mut().spawn((
        RigidBody::Dynamic,
        Velocity::default(),
        Collider::ball(1.0),
    )).id();

    app.world_mut().spawn(GearJoint {
        entity_a,
        entity_b,
        ratio: 2.0,
    });

    // Run a few updates to let Rapier calculate mass properties
    for _ in 0..10 {
        app.update();
    }

    // Check if mass props are there
    assert!(app.world().get::<ReadMassProperties>(entity_a).is_some(), "ReadMassProperties not found on A");

    // Set velocities
    if let Some(mut vel_a) = app.world_mut().get_mut::<Velocity>(entity_a) {
        vel_a.angvel = 10.0;
        vel_a.linvel = Vec2::ZERO;
    }
    if let Some(mut vel_b) = app.world_mut().get_mut::<Velocity>(entity_b) {
        vel_b.angvel = 0.0;
        vel_b.linvel = Vec2::ZERO;
    }

    // Run update (should trigger FixedUpdate and solve_gears)
    app.update();

    let vel_a = app.world().get::<Velocity>(entity_a).unwrap().angvel;
    let vel_b = app.world().get::<Velocity>(entity_b).unwrap().angvel;

    // Constraint: w_a + 2*w_b = 0
    let error = vel_a + 2.0 * vel_b;

    // With impulse solver, it should be exact if mass is finite.
    println!("vel_a: {}, vel_b: {}, error: {}", vel_a, vel_b, error);
    assert!(error.abs() < 1e-3, "Constraint failed: error={}", error);
}
