//! Cursor position tracking.
//!
//! Tracks the mouse cursor's world position, accounting for camera transforms and UI blocking.

use crate::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

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
    let ctx = contexts.ctx_mut();
    if ctx.is_pointer_over_area() {
        cursor_pos.0 = None;
        return;
    }

    if let Some(screen_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            cursor_pos.0 = Some(world_pos);
        } else {
            cursor_pos.0 = None;
        }
    } else {
        cursor_pos.0 = None;
    }
}
