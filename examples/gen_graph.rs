// Run with: cargo run --example gen_graph > schedule.dot
use bevy::prelude::*;
use bevy_mod_debugdump::schedule_graph::Settings;
use gradiance::GamePlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.build().disable::<bevy::log::LogPlugin>()); // Disable noise

    app.add_plugins(GamePlugin);

    // Print the Update schedule to stdout
    bevy_mod_debugdump::print_schedule_graph(&mut app, Update);
}
