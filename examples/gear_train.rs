//! Example of a gear train with analytic kinematic constraints.
//!
//! Press 'G' to spawn a gear train (A:20t -> B:40t -> C:10t).

use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::geometry::gear::GearProfile;
use gradiance::input::commands::{GameCommand, SpawnGearCommand};
use gradiance::physics::constraints::{GearJoint, GearPhysicsSettings};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(gradiance::GamePlugin)
        .add_systems(Update, spawn_example_gear_train)
        .run();
}

fn spawn_example_gear_train(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyG) {
        commands.queue(|world: &mut World| {
            // Enable analytic constraints
            if let Some(mut settings) = world.get_resource_mut::<GearPhysicsSettings>() {
                settings.analytic_enabled = true;
            }

            let m = 10.0;
            let z_a = 20;
            let z_b = 40;
            let z_c = 10;

            let pos_a = Vec2::new(0.0, 0.0);
            let r_a = m * z_a as f32 / 2.0;

            let r_b = m * z_b as f32 / 2.0;
            let dist_ab = r_a + r_b;
            let pos_b = pos_a + Vec2::new(dist_ab, 0.0);

            let r_c = m * z_c as f32 / 2.0;
            let dist_bc = r_b + r_c;
            // Place C above B
            let pos_c = pos_b + Vec2::new(0.0, dist_bc);

            let mut cmd_a = SpawnGearCommand {
                position: pos_a,
                profile: GearProfile::new(m, z_a, 20.0),
                entity: None,
                pin_entity: None,
            };
            if let Err(e) = cmd_a.apply(world) { error!("{}", e); return; }
            let entity_a = cmd_a.entity.unwrap();

            let mut cmd_b = SpawnGearCommand {
                position: pos_b,
                profile: GearProfile::new(m, z_b, 20.0),
                entity: None,
                pin_entity: None,
            };
            if let Err(e) = cmd_b.apply(world) { error!("{}", e); return; }
            let entity_b = cmd_b.entity.unwrap();

            let mut cmd_c = SpawnGearCommand {
                position: pos_c,
                profile: GearProfile::new(m, z_c, 20.0),
                entity: None,
                pin_entity: None,
            };
            if let Err(e) = cmd_c.apply(world) { error!("{}", e); return; }
            let entity_c = cmd_c.entity.unwrap();

            // Add Joints
            world.spawn(GearJoint {
                entity_a,
                entity_b,
                ratio: (z_b as f64) / (z_a as f64),
            });

            world.spawn(GearJoint {
                entity_a: entity_b,
                entity_b: entity_c,
                ratio: (z_c as f64) / (z_b as f64),
            });

            info!("Spawned Example Gear Train (A:20t, B:40t, C:10t)");
        });
    }
}
