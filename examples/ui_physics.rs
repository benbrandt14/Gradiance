use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use bevy_rapier2d::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, (setup_graphics, setup_physics))
        .add_systems(Update, ui_system)
        .run();
}

fn setup_graphics(mut commands: Commands) {
    commands.spawn(Camera2d);
}

#[derive(Component)]
struct Editable;

fn setup_physics(mut commands: Commands) {
    // Ground
    commands
        .spawn((
            RigidBody::Fixed,
            Collider::cuboid(500.0, 50.0),
            Transform::from_xyz(0.0, -300.0, 0.0),
        ));

    // Editable Object
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(50.0, 50.0),
        Transform::from_xyz(0.0, 200.0, 0.0),
        Restitution::coefficient(0.5),
        Friction::coefficient(0.5),
        Editable,
    ));
}

fn ui_system(
    mut contexts: EguiContexts,
    mut query: Query<(&mut Restitution, &mut Friction, &mut Transform), With<Editable>>,
) {
    let mut ctx = contexts.ctx_mut();

    egui::Window::new("Physics Inspector").show(&mut ctx, |ui| {
        ui.heading("Inspect RigidBody");

        for (mut restitution, mut friction, mut transform) in query.iter_mut() {
            ui.horizontal(|ui| {
                ui.label("Restitution:");
                ui.add(egui::Slider::new(&mut restitution.coefficient, 0.0..=2.0));
            });

            ui.horizontal(|ui| {
                ui.label("Friction:");
                ui.add(egui::Slider::new(&mut friction.coefficient, 0.0..=2.0));
            });

            if ui.button("Reset Position").clicked() {
                transform.translation = Vec3::new(0.0, 200.0, 0.0);
                transform.rotation = Quat::IDENTITY;
            }
        }

        ui.label("Modify the properties and watch the box interact with the ground.");
    });
}
