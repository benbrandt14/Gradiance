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
    mut query: Query<&mut Transform, With<Camera3d>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    // select_tool_data: Option<Res<SelectToolData>>,
) {
    // Check if Right or Middle mouse button is held
    if !mouse_buttons.pressed(MouseButton::Right) && !mouse_buttons.pressed(MouseButton::Middle) {
        return;
    }

    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    for mut transform in query.iter_mut() {
        // Simple scaling based on Z distance (perspective zoom)
        // At Z=60, scale factor should allow reasonable panning speed.
        // Screen width ~ Z * tan(FOV/2).
        // Let's assume Z around 60.
        // Sensitivity 0.1 works for orthographic scale 1.0.
        // For perspective, world movement = delta * (Z / height_pixels * 2 * tan(fov/2))

        // Approximate factor:
        let sensitivity = transform.translation.z.abs() * 0.002;

        // Invert X and Y for drag-to-move-world feel
        transform.translation.x -= delta.x * sensitivity;
        transform.translation.y += delta.y * sensitivity;
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
        let mut z = transform.translation.z;

        // Zoom speed
        let sensitivity = 0.1;

        if scroll > 0.0 {
            z /= 1.0 + sensitivity;
        } else {
            z *= 1.0 + sensitivity;
        }

        // Clamp zoom
        z = z.clamp(5.0, 1000.0);

        transform.translation.z = z;
    }
}
