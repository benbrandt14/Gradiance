use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_rapier2d::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, spawn_on_click)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Ground
    commands
        .spawn((
            RigidBody::Fixed,
            Collider::cuboid(500.0, 50.0),
            Transform::from_xyz(0.0, -300.0, 0.0),
        ));

    // Instructions
    commands.spawn((
        Text::new("Click to Spawn Boxes"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}

fn spawn_on_click(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
) {
    if mouse_button.just_pressed(MouseButton::Left) {
        let (camera, camera_transform) = camera_q.single();
        let window = windows.single();

        if let Some(world_position) = window
            .cursor_position()
            .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        {
            commands.spawn((
                RigidBody::Dynamic,
                Collider::cuboid(20.0, 20.0),
                Transform::from_xyz(world_position.x, world_position.y, 0.0),
                Restitution::coefficient(0.7),
            ));
            info!("Spawned box at {:?}", world_position);
        }
    }
}
