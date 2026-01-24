use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier::parry::shape::TypedShape;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, handle_snapping)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Ground
    commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(500.0, 50.0),
        Transform::from_xyz(0.0, -300.0, 0.0),
    ));

    // Box
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(50.0, 50.0),
        Transform::from_xyz(-100.0, 0.0, 0.0),
    ));

    // Polygon (Triangle)
    let vertices = vec![
        Vec2::new(-50.0, -50.0),
        Vec2::new(50.0, -50.0),
        Vec2::new(0.0, 50.0),
    ];
    commands.spawn((
        RigidBody::Dynamic,
        Collider::convex_hull(&vertices).unwrap(),
        Transform::from_xyz(100.0, 0.0, 0.0),
    ));

    // Instructions
    commands.spawn((
        Text::new("Hover over objects to snap to corners/center"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}

fn handle_snapping(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    rapier_context: Query<&RapierContext>,
    colliders: Query<(&Collider, &GlobalTransform)>,
    mut gizmos: Gizmos,
) {
    let window = windows.single();
    let (camera, camera_transform) = camera_q.single();

    if let Some(cursor_pos) = window.cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
    {
        // 1. Find entity under cursor
        let rapier = rapier_context.single();
        let mut hit_entity = None;

        rapier.intersections_with_point(cursor_pos, QueryFilter::default(), |entity| {
            hit_entity = Some(entity);
            false // Stop at first
        });

        if let Some(entity) = hit_entity {
            if let Ok((collider, transform)) = colliders.get(entity) {
                let mut snap_points = Vec::new();

                // Add Center
                let center = transform.translation().truncate();
                snap_points.push(center);

                // Analyze Shape
                // Note: transform rotation/scale must be applied to local points
                // GlobalTransform handles the full transform hierarchy, so we can transform local points to world.

                let raw_collider = &collider.raw;

                match raw_collider.as_typed_shape() {
                    TypedShape::Cuboid(c) => {
                        let h = Vec2::new(c.half_extents.x, c.half_extents.y);
                        // 4 Corners
                        let corners = [
                            Vec2::new(h.x, h.y),
                            Vec2::new(-h.x, h.y),
                            Vec2::new(h.x, -h.y),
                            Vec2::new(-h.x, -h.y),
                        ];
                        for local_p in corners {
                            let world_p = transform.transform_point(local_p.extend(0.0)).truncate();
                            snap_points.push(world_p);

                            // Midpoints
                            // We can add midpoints of edges too
                        }

                        // Midpoints (Top, Bottom, Left, Right)
                        let midpoints = [
                             Vec2::new(0.0, h.y),
                             Vec2::new(0.0, -h.y),
                             Vec2::new(h.x, 0.0),
                             Vec2::new(-h.x, 0.0),
                        ];
                         for local_p in midpoints {
                            let world_p = transform.transform_point(local_p.extend(0.0)).truncate();
                            snap_points.push(world_p);
                        }
                    },
                    TypedShape::ConvexPolygon(p) => {
                        // Vertices
                        for point in p.points() {
                            let local_p = Vec2::new(point.x, point.y);
                            let world_p = transform.transform_point(local_p.extend(0.0)).truncate();
                            snap_points.push(world_p);
                        }
                    },
                    TypedShape::Triangle(t) => {
                         let points = [t.a, t.b, t.c];
                         for point in points {
                            let local_p = Vec2::new(point.x, point.y);
                            let world_p = transform.transform_point(local_p.extend(0.0)).truncate();
                            snap_points.push(world_p);
                        }
                    }
                    _ => {}
                }

                // Find closest snap point
                let mut best_point = cursor_pos;
                let mut min_dist_sq = f32::MAX;
                let snap_threshold = 20.0; // Snap distance

                for p in snap_points {
                    let d2 = p.distance_squared(cursor_pos);
                    if d2 < min_dist_sq {
                        min_dist_sq = d2;
                        best_point = p;
                    }
                    // Visualise all candidates faintly
                    gizmos.circle_2d(p, 2.0, Color::srgb(0.5, 0.5, 0.5));
                }

                if min_dist_sq < snap_threshold * snap_threshold {
                    // Snap!
                    gizmos.circle_2d(best_point, 5.0, Color::srgb(0.0, 1.0, 0.0));
                    gizmos.line_2d(cursor_pos, best_point, Color::srgb(0.0, 1.0, 0.0));
                } else {
                    // Show cursor
                    gizmos.circle_2d(cursor_pos, 3.0, Color::WHITE);
                }
            }
        }
    }
}
