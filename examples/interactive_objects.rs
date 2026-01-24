use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, (setup_graphics, setup_physics))
        .add_systems(Update, (player_movement, display_events))
        .run();
}

fn setup_graphics(mut commands: Commands) {
    commands.spawn(Camera2d);
}

#[derive(Component)]
struct Player;

fn setup_physics(mut commands: Commands) {
    // Ground
    commands
        .spawn((
            RigidBody::Fixed,
            Collider::cuboid(500.0, 50.0),
            Transform::from_xyz(0.0, -300.0, 0.0),
        ));

    // Player
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(30.0, 30.0),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Velocity::default(),
        LockedAxes::ROTATION_LOCKED, // Keep player upright
        Player,
    ));

    // Obstacles
    for i in -5..5 {
        commands.spawn((
            RigidBody::Dynamic,
            Collider::ball(15.0),
            Transform::from_xyz(i as f32 * 50.0, 200.0 + i as f32 * 20.0, 0.0),
            Restitution::coefficient(0.7),
        ));
    }

    // UI Instructions
    commands.spawn((
        Text::new("Use Arrow Keys to Move. Space to Jump."),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}

fn player_movement(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Velocity, With<Player>>,
) {
    for mut velocity in query.iter_mut() {
        let speed = 200.0;
        let jump_force = 500.0;

        if input.pressed(KeyCode::ArrowLeft) {
            velocity.linvel.x = -speed;
        } else if input.pressed(KeyCode::ArrowRight) {
            velocity.linvel.x = speed;
        } else {
            velocity.linvel.x = 0.0;
        }

        if input.just_pressed(KeyCode::Space) {
            velocity.linvel.y = jump_force;
        }
    }
}

fn display_events(
    mut collision_events: EventReader<CollisionEvent>,
) {
    for _collision_event in collision_events.read() {
        // Just consume events to prevent overflow warnings in logs if enabled
    }
}
