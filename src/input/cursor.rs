//! Cursor position tracking.
//!
//! Tracks the mouse cursor's world position, accounting for camera transforms and UI blocking.

use crate::prelude::*;
use bevy::math::DVec2;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

/// Custom cursor position resource.
///
/// Stores the world-space coordinates of the mouse cursor.
///
/// We implement this manually instead of using a plugin like `bevy_cursor` because:
/// 1. We need `DVec2` (f64) precision for Avian2d physics.
/// 2. We need tight integration with `bevy_egui` to block cursor updates when hovering UI.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorldPos(pub Option<DVec2>);

/// Updates the `CursorWorldPos` resource.
///
/// Projects the window cursor position to world coordinates.
/// Returns `None` if the cursor is over an Egui UI area.
pub fn update_cursor_pos(
    mut cursor_pos: ResMut<CursorWorldPos>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform)>,
    mut contexts: EguiContexts,
) {
    // Use iter().next() instead of get_single() as per AGENTS.md/memory
    let Some(window) = q_window.iter().next() else {
        return;
    };
    let Some((camera, camera_transform)) = q_camera.iter().next() else {
        return;
    };

    // Check if mouse is over UI
    // In Bevy 0.18 / Egui 0.39, ctx_mut might return a Result.
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.is_pointer_over_area() {
            cursor_pos.0 = None;
            return;
        }

    if let Some(screen_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            cursor_pos.0 = Some(DVec2::new(world_pos.x as f64, world_pos.y as f64));
        } else {
            cursor_pos.0 = None;
        }
    } else {
        cursor_pos.0 = None;
    }
}
