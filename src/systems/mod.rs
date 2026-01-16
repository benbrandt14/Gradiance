pub mod camera;
pub mod physics;

use bevy::prelude::*;
use bevy_firefly::prelude::*;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        camera::PanCam::default(),
        FireflyConfig::default(),
    ));

    commands.spawn(PointLight2d {
        color: Color::WHITE,
        intensity: 2.0,
        range: 1000.0,
        ..default()
    });
}
