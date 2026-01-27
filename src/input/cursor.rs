//! Cursor position tracking.
//!
//! Tracks the mouse cursor's world position, accounting for camera transforms and UI blocking.

use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

// Import from snapping module
use crate::input::snapping::{SnapSource, SnappingStatus, calculate_snapping};

/// Custom cursor position resource.
///
/// Stores the world-space coordinates of the mouse cursor.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorldPos(pub Option<Vec2>);

/// Updates the `CursorWorldPos` resource.
///
/// Projects the window cursor position to world coordinates.
/// Returns `None` if the cursor is over an Egui UI area.
pub fn update_cursor_pos(
    mut cursor_pos: ResMut<CursorWorldPos>,
    mut snap_status: ResMut<SnappingStatus>,
    grid_settings: Res<GridSettings>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut contexts: EguiContexts,
    // Physics queries for object snapping
    q_rapier: Query<&RapierContext>,
    q_collider: Query<(&Collider, &GlobalTransform)>,
) {
    // Reset snap status
    snap_status.snapped = false;
    snap_status.snap_source = None;
    cursor_pos.0 = None; // Reset cursor pos

    // Use iter().next() instead of get_single() as per AGENTS.md/memory
    let Some(window) = q_window.iter().next() else {
        return;
    };
    let Some((camera, camera_transform)) = q_camera.iter().next() else {
        return;
    };

    // Check if mouse is over UI
    let ctx = contexts.ctx_mut();
    if ctx.is_pointer_over_area() {
        return;
    }

    let mut raw_pos = None;

    // 3D Raycasting to Z=0 plane
    if let Some(screen_pos) = window.cursor_position()
        && let Ok(ray) = camera.viewport_to_world(camera_transform, screen_pos) {
            // Ray: origin + t * direction
            // Plane: Z=0. Normal: (0,0,1). Point: (0,0,0).
            // Distance t = (point - origin) . normal / (direction . normal)
            // t = (0 - origin.z) / direction.z

            let normal = Vec3::Z;
            let denominator = ray.direction.dot(normal);

            if denominator.abs() > 0.0001 {
                let t = (0.0 - ray.origin.z) / ray.direction.z;
                if t >= 0.0 {
                    let point = ray.origin + ray.direction * t;
                    raw_pos = Some(Vec2::new(point.x, point.y));
                }
            }
        }

    if let Some(pos) = raw_pos {
        cursor_pos.0 = Some(pos);
    } else {
        return;
    }

    let rapier_context = q_rapier.iter().next();

    // Need explicit callback for snapping closure to avoid lifetime issues
    let get_collider_wrapper = |entity: Entity| -> Option<(Collider, GlobalTransform)> {
        q_collider.get(entity).ok().map(|(c, t)| (c.clone(), *t))
    };

    let (best_snap_pos, snapped, source) = if let Some(rapier_context) = rapier_context {
        calculate_snapping(
            raw_pos.unwrap(),
            &grid_settings,
            Some(|aabb, callback: &mut dyn FnMut(Entity) -> bool| {
                rapier_context.colliders_with_aabb_intersecting_aabb(aabb, callback);
            }),
            get_collider_wrapper,
        )
    } else {
        // Explicitly annotate type of None for spatial query
        // Using explicit function type to satisfy type inference
        calculate_snapping(
            raw_pos.unwrap(),
            &grid_settings,
            None::<fn(bevy::math::bounding::Aabb2d, &mut dyn FnMut(Entity) -> bool)>,
            get_collider_wrapper,
        )
    };

    if snapped {
        cursor_pos.0 = Some(best_snap_pos);
        snap_status.snapped = true;
        snap_status.snap_pos = best_snap_pos;
        snap_status.snap_source = source;
    }
}

/// System to visualize the snapping point.
pub fn draw_snap_indicators(
    snap_status: Res<SnappingStatus>,
    mut gizmos: Gizmos,
    _q_camera: Query<&Projection, With<Camera3d>>,
) {
    if !snap_status.snapped {
        return;
    }

    let pos = snap_status.snap_pos;
    let color = match snap_status.snap_source {
        Some(SnapSource::Grid) => Color::srgb(0.0, 1.0, 0.0), // Green
        Some(_) => Color::srgb(0.0, 1.0, 1.0),                // Cyan
        None => Color::WHITE,
    };

    let radius = 1.0;

    // Draw circle on Z=0 plane
    gizmos.circle(
        Isometry3d::new(Vec3::new(pos.x, pos.y, 0.0), Quat::IDENTITY),
        radius,
        color,
    );

    // Crosshair
    let arm_len = radius * 2.0;
    gizmos.line(
        Vec3::new(pos.x - arm_len, pos.y, 0.0),
        Vec3::new(pos.x + arm_len, pos.y, 0.0),
        color,
    );
    gizmos.line(
        Vec3::new(pos.x, pos.y - arm_len, 0.0),
        Vec3::new(pos.x, pos.y + arm_len, 0.0),
        color,
    );
}
