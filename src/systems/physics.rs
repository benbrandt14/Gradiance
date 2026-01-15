use avian2d::prelude::*;
use bevy::prelude::*;

pub fn spawn_ground(mut commands: Commands) {
    commands.spawn((
        RigidBody::Static,
        Collider::rectangle(1000.0, 20.0),
        Transform::from_xyz(0.0, -300.0, 0.0),
        Sprite::from_color(Color::WHITE, Vec2::new(1000.0, 20.0)),
    ));
}
