use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use std::collections::VecDeque;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_systems(Startup, setup)
        .add_systems(Update, (update_trails, draw_trails))
        .run();
}

#[derive(Component)]
struct Trail {
    points: VecDeque<Vec2>,
    max_length: usize,
    color: Color,
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Ground
    commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(500.0, 50.0),
        Transform::from_xyz(0.0, -300.0, 0.0),
    ));

    // Simulated Light (Visual)
    commands.spawn((
        Sprite {
            color: Color::srgba(1.0, 0.9, 0.5, 0.3),
            custom_size: Some(Vec2::splat(300.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 200.0, -1.0),
    ));

    // Ball with Trail
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(20.0),
        Transform::from_xyz(0.0, 200.0, 0.0),
        Restitution::coefficient(0.8),
        Trail {
            points: VecDeque::new(),
            max_length: 50,
            color: Color::srgb(1.0, 0.5, 0.0),
        },
    ));

    // Another Ball
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(15.0),
        Transform::from_xyz(100.0, 250.0, 0.0),
        Restitution::coefficient(0.9),
        Trail {
            points: VecDeque::new(),
            max_length: 50,
            color: Color::srgb(0.0, 0.5, 1.0),
        },
    ));

    commands.spawn((
        Text::new("Gizmo Trails (Tracers)"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}

fn update_trails(
    mut query: Query<(&GlobalTransform, &mut Trail)>,
) {
    for (transform, mut trail) in query.iter_mut() {
        let pos = transform.translation().truncate();
        trail.points.push_back(pos);
        if trail.points.len() > trail.max_length {
            trail.points.pop_front();
        }
    }
}

fn draw_trails(
    mut gizmos: Gizmos,
    query: Query<&Trail>,
) {
    for trail in query.iter() {
        if trail.points.len() < 2 {
            continue;
        }

        // Draw line strip
        // Gizmos doesn't have `line_strip_2d`? It uses `linestrip_2d` or we iterate.
        // Bevy 0.15 Gizmos has `linestrip_2d`.
        gizmos.linestrip_2d(trail.points.iter().copied(), trail.color);
    }
}
