use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::sprite::Anchor;

pub fn spawn_ground(mut commands: Commands) {
    commands.spawn((
        RigidBody::Static,
        Collider::half_space(Vec2::Y),
        Friction::default(),
        Restitution::new(0.5),
        Transform::from_xyz(0.0, -500.0, 0.0),
        Sprite {
            color: Color::WHITE,
            custom_size: Some(Vec2::new(100000.0, 1000.0)),
            ..default()
        },
        Anchor::default(), // Center anchor as component
    ));
}
