//! Camera controller for panning and zooming.
//!
//! Provides WASD movement, Mouse Drag panning, and Scroll zooming.

use crate::input::tools::select_tool::SelectToolData;
use crate::prelude::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};

/// Plugin for camera control.
pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (camera_pan, camera_zoom));
    }
}

/// Pans the camera when Right or Middle mouse button is dragged.
pub fn camera_pan(
    mut query: Query<(&mut Transform, &Camera), With<Camera3d>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    select_tool_data: Option<Res<SelectToolData>>,
) {
    // Check if Right or Middle mouse button is held
    if !mouse_buttons.pressed(MouseButton::Right) && !mouse_buttons.pressed(MouseButton::Middle) {
        return;
    }

    // If rotating with Select Tool, ignore Right Click panning
    if mouse_buttons.pressed(MouseButton::Right)
        && let Some(data) = &select_tool_data
        && data.is_rotating
    {
        return;
    }

    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    for (mut transform, _camera) in query.iter_mut() {
        // For perspective camera, we move along the local Right and Up vectors (projected to move broadly parallel to ground).
        // Since the camera is tilted, "Up" on screen is not world Z or world Y.
        // Screen Y movement should move the camera such that the ground point moves roughly with the mouse.
        // A simple approximation is to move along the camera's local Right (-X) and local Up (Y).

        // Speed factor: proportional to distance (Z height approx)
        let distance = transform.translation.z.abs().max(1.0);
        let speed = distance * 0.001; // Tunable

        let right = transform.right();
        let up = transform.up();

        let movement = (right * -delta.x + up * delta.y) * speed;
        transform.translation += movement;
    }
}

fn camera_zoom(
    mut query: Query<&mut Transform, With<Camera3d>>,
    scroll_events: Res<AccumulatedMouseScroll>,
) {
    let scroll = scroll_events.delta.y;
    if scroll == 0.0 {
        return;
    }

    for mut transform in query.iter_mut() {
        // Zoom by moving forward/backward
        let sensitivity = 0.1;
        let forward = transform.forward();

        // Move along forward vector
        // If scroll > 0 (zoom in), move forward.
        let amount = if scroll > 0.0 {
            1.0 * sensitivity * transform.translation.length() // Speed proportional to distance
        } else {
            -1.0 * sensitivity * transform.translation.length()
        };

        let movement = forward * amount;

        // Check bounds (don't go through origin)
        let new_pos = transform.translation + movement;
        if new_pos.length() > 2.0 && new_pos.length() < 500.0 {
             transform.translation = new_pos;
        }
    }
}
