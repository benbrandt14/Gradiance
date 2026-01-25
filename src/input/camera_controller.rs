//! Camera controller for panning and zooming.
//!
//! Provides WASD movement, Mouse Drag panning, and Scroll zooming.

use crate::prelude::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use crate::input::tools::select_tool::SelectToolData;

/// Plugin for camera control.
pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (camera_pan, camera_zoom));
    }
}

/// Pans the camera when Right or Middle mouse button is dragged.
pub fn camera_pan(
    mut query: Query<(&mut Transform, &Camera), With<Camera2d>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    select_tool_data: Option<Res<SelectToolData>>,
) {
    // Check if Right or Middle mouse button is held
    if !mouse_buttons.pressed(MouseButton::Right) && !mouse_buttons.pressed(MouseButton::Middle) {
        return;
    }

    // If rotating with Select Tool, ignore Right Click panning
    if mouse_buttons.pressed(MouseButton::Right) {
        if let Some(data) = &select_tool_data {
            if data.is_rotating {
                return;
            }
        }
    }

    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    for (mut transform, _camera) in query.iter_mut() {
        // We need to scale the delta by the current zoom level (scale.x)
        // to keep panning speed consistent relative to the world.
        // Also invert x and y because dragging mouse right means moving camera left to keep world "under" cursor.
        let zoom = transform.scale.x;

        // However, we want to move the camera opposite to mouse drag to "grab" the world.
        transform.translation.x -= delta.x * zoom;
        transform.translation.y += delta.y * zoom;
    }
}

fn camera_zoom(
    mut query: Query<&mut Transform, With<Camera2d>>,
    scroll_events: Res<AccumulatedMouseScroll>,
) {
    let scroll = scroll_events.delta.y;
    if scroll == 0.0 {
        return;
    }

    for mut transform in query.iter_mut() {
        let mut scale = transform.scale.x;

        // Zoom speed
        let sensitivity = 0.1;
        if scroll > 0.0 {
            scale /= 1.0 + sensitivity;
        } else {
            scale *= 1.0 + sensitivity;
        }

        // Clamp zoom
        scale = scale.clamp(0.01, 1000.0);

        transform.scale = Vec3::splat(scale);
    }
}
