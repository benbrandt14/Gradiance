pub mod systems;
pub mod components;
pub mod resources;
pub mod ui;
pub mod tools;
pub mod commands;

use bevy::prelude::*;
use avian2d::prelude::*;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gradiance".into(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default().with_length_unit(20.0))
        .add_plugins(PhysicsDebugPlugin::default())
        .add_plugins(tools::ToolPlugin)
        .add_plugins(ui::UiPlugin)
        .init_resource::<commands::CommandStack>()
        .add_systems(Startup, (systems::setup, systems::physics::spawn_ground))
        .add_systems(Update, systems::camera::camera_movement);
    }
}
