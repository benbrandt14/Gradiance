use bevy::prelude::*;
use gradiance::GamePlugin;

fn main() {
    App::new()
        .add_plugins(GamePlugin)
        .run();
}
