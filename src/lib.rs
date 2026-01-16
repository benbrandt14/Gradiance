pub mod commands;
pub mod components;
pub mod resources;
pub mod systems;
pub mod tools;
pub mod ui;

// TODO: Future Modules:
// pub mod scripting;
// pub mod csg;
// pub mod lasers;
// pub mod mechanisms; // Thrusters, Hinges, Springs

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_firefly::prelude::*;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gradiance".into(),
                // Resolution using u32 integers as per 0.17 environment requirement
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default().with_length_unit(20.0))
        .add_plugins(PhysicsDebugPlugin::default())
        .add_plugins(FireflyPlugin)
        .add_plugins(tools::ToolPlugin)
        .add_plugins(ui::UiPlugin)
        // TODO: Add ScriptingPlugin
        // TODO: Add CsgPlugin
        // TODO: Add LasersPlugin
        .init_resource::<commands::CommandStack>()
        .add_systems(Startup, (systems::setup, systems::physics::spawn_ground))
        .add_systems(Update, systems::camera::camera_movement);
    }
}
