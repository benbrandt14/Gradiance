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
    mut query: Query<(&mut Transform, &Projection), With<Camera>>,
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

    for (mut transform, projection) in query.iter_mut() {
        match projection {
            Projection::Orthographic(_) => {
                // 2D Logic
                let zoom = transform.scale.x;
                transform.translation.x -= delta.x * zoom;
                transform.translation.y += delta.y * zoom;
            }
            Projection::Perspective(_) => {
                // 3D Logic
                // Scale pan speed by distance to ground (approx z height)
                let dist = transform.translation.z.max(1.0);
                let speed = dist * 0.002; // Tune this factor

                transform.translation.x -= delta.x * speed;
                transform.translation.y += delta.y * speed;
            }
        }
    }
}

fn camera_zoom(
    mut query: Query<(&mut Transform, &Projection), With<Camera>>,
    scroll_events: Res<AccumulatedMouseScroll>,
) {
    let scroll = scroll_events.delta.y;
    if scroll == 0.0 {
        return;
    }

    for (mut transform, projection) in query.iter_mut() {
        // Zoom speed
        let sensitivity = 0.1;

        match projection {
            Projection::Orthographic(_) => {
                // 2D Logic
                let mut scale = transform.scale.x;
                if scroll > 0.0 {
                    scale /= 1.0 + sensitivity;
                } else {
                    scale *= 1.0 + sensitivity;
                }
                scale = scale.clamp(0.01, 1000.0);
                transform.scale = Vec3::splat(scale);
            }
            Projection::Perspective(_) => {
                // 3D Logic: Move along forward vector
                let forward = transform.forward();
                let mut zoom_amount = scroll * sensitivity * transform.translation.z.abs(); // Scale step by distance

                // Limit zoom in (don't clip through ground)
                if transform.translation.z < 2.0 && zoom_amount > 0.0 {
                    zoom_amount = 0.0;
                }

                transform.translation += forward * zoom_amount;
            }
        }
    }
}
