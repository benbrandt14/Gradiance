pub mod camera;
pub mod physics;

use bevy::prelude::*;

pub fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, camera::PanCam::default()));
}
