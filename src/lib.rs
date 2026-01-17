pub mod prelude;
pub mod physics;
pub mod geometry;
pub mod input;
pub mod ui;
pub mod scripting;

use bevy::prelude::*;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            physics::PhysicsPlugin,
            geometry::GeometryPlugin,
            input::InputPlugin,
            ui::UiPlugin,
            scripting::ScriptingPlugin,
        ));
    }
}
