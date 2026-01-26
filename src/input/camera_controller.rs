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
        app.add_systems(Update, (camera_pan_orbit, camera_zoom));
    }
}

/// Pans the camera when Right mouse button is dragged.
/// Orbits when Middle mouse button is dragged.
pub fn camera_pan_orbit(
    mut query: Query<(&mut Transform, &Camera), With<Camera3d>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    select_tool_data: Option<Res<SelectToolData>>,
) {
    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    // Panning (Right Click)
    if mouse_buttons.pressed(MouseButton::Right) {
        // If rotating with Select Tool, ignore Right Click panning
        if let Some(data) = &select_tool_data {
            if data.is_rotating {
                return;
            }
        }

        for (mut transform, _camera) in query.iter_mut() {
            // Speed factor: proportional to distance (Z height approx)
            let distance = transform.translation.length().max(1.0);
            let speed = distance * 0.0005; // Tunable

            let right = transform.right();
            let up = transform.up();

            let movement = (right * -delta.x + up * delta.y) * speed;
            transform.translation += movement;
        }
    }
    // Orbiting (Middle Click or Alt+Left)
    else if mouse_buttons.pressed(MouseButton::Middle) {
         for (mut transform, _camera) in query.iter_mut() {
            let sensitivity = 0.005;
            let rotation_x = Quat::from_rotation_y(-delta.x * sensitivity);
            let rotation_y = Quat::from_rotation_x(-delta.y * sensitivity);

            // Orbit around a pivot point.
            // Ideally pivot is cursor position or (0,0,0) or center of screen.
            // Simple orbit: Rotate around (0,0,0) on ground?
            // Or rotate around "LookAt" point.
            // Let's assume looking at 0,0,0 or just rotate camera around its focus distance.

            // For now, simpler "Editor Camera":
            // Rotate around the camera's current focus point (approx distance forward).
            let focus_dist = transform.translation.length();
            let focus_point = transform.translation + transform.forward() * focus_dist; // Roughly 0,0,0 if looking at it

            // Actually, usually we look at 0,0,0.
            // Let's just rotate position around 0,0,0?
            // No, that restricts movement.

            // Better: Rotate transforms around focus point.
            // But we don't track focus point.
            // Let's assume focus is some distance away along forward vector.
            let focus = transform.translation + transform.forward() * transform.translation.length(); // if we look at 0,0,0 from start

            // Translate to origin (relative to focus)
            let offset = transform.translation - focus;

            // Apply rotations
            // Rotate around global Y for yaw (delta x)
            let offset = rotation_x * offset;

            // Rotate around local X for pitch (delta y)
            // Need to be careful with Gimbal lock or up vector.
            // Local Right axis
            let right = transform.right();
            let pitch_rot = Quat::from_axis_angle(*right, -delta.y * sensitivity);
            let offset = pitch_rot * offset;

            transform.translation = focus + offset;
            transform.look_at(focus, Vec3::Y); // Keep Y up
         }
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
        if new_pos.length() > 2.0 && new_pos.length() < 1000.0 {
             transform.translation = new_pos;
        }
    }
}
