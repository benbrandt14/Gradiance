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
    q_camera: Query<(&Camera, &GlobalTransform)>,
    mut contexts: EguiContexts,
    // Physics queries for object snapping
    q_rapier: Query<&RapierContext>,
    q_collider: Query<(&Collider, &GlobalTransform)>,
) {
    // Reset snap status
    snap_status.snapped = false;
    snap_status.snap_source = None;

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
        cursor_pos.0 = None;
        return;
    }

    let mut raw_pos = None;
    if let Some(screen_pos) = window.cursor_position() {
        // Try 2D first (for backward compat if Camera2d is used)
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            raw_pos = Some(world_pos);
        } else if let Ok(ray) = camera.viewport_to_world(camera_transform, screen_pos) {
            // Raycast to Z=0 plane
            let normal = Vec3::Z;
            let origin = Vec3::ZERO;
            let denom = ray.direction.dot(normal);
            if denom.abs() > f32::EPSILON {
                let t = (origin - ray.origin).dot(normal) / denom;
                if t >= 0.0 {
                    let point = ray.get_point(t);
                    raw_pos = Some(point.truncate());
                }
            }
        }
    }

    cursor_pos.0 = raw_pos;

    let Some(raw) = raw_pos else {
        return;
    };

    let get_collider_wrapper = |entity| q_collider.get(entity).ok().map(|(c, t)| (c.clone(), *t));

    let (best_snap_pos, snapped, snap_source) = if let Some(rapier_context) = q_rapier.iter().next()
    {
        calculate_snapping(
            raw,
            &grid_settings,
            Some(|aabb, callback: &mut dyn FnMut(Entity) -> bool| {
                rapier_context.colliders_with_aabb_intersecting_aabb(aabb, callback);
            }),
            get_collider_wrapper,
        )
    } else {
        // No rapier context, pass None for spatial query
        calculate_snapping(
            raw,
            &grid_settings,
            None::<fn(bevy::math::bounding::Aabb2d, &mut dyn FnMut(Entity) -> bool)>, // Explicit type for None
            get_collider_wrapper,
        )
    };

    if snapped {
        cursor_pos.0 = Some(best_snap_pos);
        snap_status.snapped = true;
        snap_status.snap_pos = best_snap_pos;
        snap_status.snap_source = snap_source;
    }
}

/// System to visualize the snapping point.
pub fn draw_snap_indicators(
    snap_status: Res<SnappingStatus>,
    mut gizmos: Gizmos,
    _q_camera: Query<&Projection, With<Camera>>,
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

    // Fixed size for now, or could query camera distance
    let radius = 0.5;

    // Draw in 3D at Z=0.1 to avoid z-fighting if coincidental
    let center_3d = Vec3::new(pos.x, pos.y, 0.1);

    // gizmos.circle takes (position, radius, color) in this version.
    // It likely draws a billboard or camera-facing circle.
    gizmos.circle(center_3d, radius, color);

    // Crosshair
    let arm_len = radius * 2.0;
    gizmos.line(
        center_3d - Vec3::new(arm_len, 0.0, 0.0),
        center_3d + Vec3::new(arm_len, 0.0, 0.0),
        color,
    );
    gizmos.line(
        center_3d - Vec3::new(0.0, arm_len, 0.0),
        center_3d + Vec3::new(0.0, arm_len, 0.0),
        color,
    );
}
