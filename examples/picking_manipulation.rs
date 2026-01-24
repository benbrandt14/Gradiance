use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .init_resource::<Selection>()
        .add_systems(Startup, setup)
        .add_systems(Update, (
            handle_picking,
            handle_drag,
            handle_scaling,
            highlight_selection,
        ))
        .run();
}

#[derive(Resource, Default)]
struct Selection {
    entity: Option<Entity>,
    drag_offset: Vec2,
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Instructions
    commands.spawn((
        Text::new("Left Click: Select/Drag\nScroll: Scale\nClick Background: Deselect"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));

    // Ground
    commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(500.0, 50.0),
        Transform::from_xyz(0.0, -300.0, 0.0),
        // Add a visual color for debug (using Sprite or Mesh? DebugRender handles colliders)
        // We'll trust DebugRender for now.
    ));

    // Manipulatable Box
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(50.0, 50.0),
        Transform::from_xyz(-100.0, 0.0, 0.0),
        Restitution::coefficient(0.7),
    ));

    // Manipulatable Ball
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(50.0),
        Transform::from_xyz(100.0, 0.0, 0.0),
        Restitution::coefficient(0.7),
    ));
}

fn handle_picking(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    rapier_context: Query<&RapierContext>,
    mut selection: ResMut<Selection>,
    transforms: Query<&GlobalTransform>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        let window = windows.single();
        let (camera, camera_transform) = camera_q.single();

        if let Some(cursor_pos) = window.cursor_position()
            .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        {
            // Point projection to find what we clicked
            // Note: In bevy_rapier 0.28+, RapierContext is a component.
            // We assume there is one context (single world).
            let rapier = rapier_context.single();
            let filter = QueryFilter::default();

            let mut hit_entity = None;

            // Iterate over bodies to check point? Or use project_point
            rapier.intersections_with_point(cursor_pos, filter, |entity| {
                hit_entity = Some(entity);
                false // Stop at first hit
            });

            if let Some(entity) = hit_entity {
                selection.entity = Some(entity);
                // Calculate offset
                if let Ok(transform) = transforms.get(entity) {
                    let pos = transform.translation().truncate();
                    selection.drag_offset = pos - cursor_pos;
                }
                info!("Selected entity {:?}", entity);
            } else {
                selection.entity = None;
            }
        }
    }
}

fn handle_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    selection: Res<Selection>,
    mut transforms: Query<&mut Transform>,
    mut bodies: Query<&mut Velocity>, // We might want to set velocity to 0 or use kinematic
) {
    if mouse.pressed(MouseButton::Left) {
        if let Some(entity) = selection.entity {
            let window = windows.single();
            let (camera, camera_transform) = camera_q.single();

            if let Some(cursor_pos) = window.cursor_position()
                .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
            {
                let target_pos = cursor_pos + selection.drag_offset;

                if let Ok(mut transform) = transforms.get_mut(entity) {
                    // Direct translation
                    transform.translation.x = target_pos.x;
                    transform.translation.y = target_pos.y;

                    // Reset velocity to avoid physics "exploding" after drag
                    if let Ok(mut velocity) = bodies.get_mut(entity) {
                        velocity.linvel = Vec2::ZERO;
                        velocity.angvel = 0.0;
                    }
                }
            }
        }
    }
}

fn handle_scaling(
    mut scroll_evr: EventReader<bevy::input::mouse::MouseWheel>,
    selection: Res<Selection>,
    mut transforms: Query<&mut Transform>,
) {
    for ev in scroll_evr.read() {
        if let Some(entity) = selection.entity {
            if let Ok(mut transform) = transforms.get_mut(entity) {
                let scale_factor = 1.0 + ev.y * 0.1;
                transform.scale *= scale_factor;
                // Clamp scale to reasonable values
                transform.scale = transform.scale.clamp(Vec3::splat(0.1), Vec3::splat(5.0));
            }
        }
    }
}

fn highlight_selection(
    mut gizmos: Gizmos,
    selection: Res<Selection>,
    transforms: Query<&GlobalTransform>,
) {
    if let Some(entity) = selection.entity {
        if let Ok(transform) = transforms.get(entity) {
            let pos = transform.translation().truncate();
            gizmos.circle_2d(pos, 60.0, Color::WHITE); // Simple highlight
        }
    }
}
