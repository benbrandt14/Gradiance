//! Cursor position tracking.
//!
//! Tracks the mouse cursor's world position.

use crate::prelude::*;
use bevy::math::DVec2;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

/// Custom cursor position resource.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorldPos(pub Option<DVec2>);

/// Updates the `CursorWorldPos` resource.
pub fn update_cursor_pos(
    mut cursor_pos: ResMut<CursorWorldPos>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform)>,
    mut contexts: EguiContexts,
) {
    let Some(window) = q_window.iter().next() else {
        return;
    };
    let Some((camera, camera_transform)) = q_camera.iter().next() else {
        return;
    };

    let ctx = contexts.ctx_mut();
    if ctx.is_pointer_over_area() {
            cursor_pos.0 = None;
            return;
        }

    if let Some(screen_pos) = window.cursor_position() {
        if let Some(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            cursor_pos.0 = Some(DVec2::new(world_pos.x as f64, world_pos.y as f64));
        } else {
            cursor_pos.0 = None;
        }
    } else {
        cursor_pos.0 = None;
    }
}
