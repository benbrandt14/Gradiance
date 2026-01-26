//! Camera controller for panning, zooming, and rotating.
//!
//! Provides WASD movement, Mouse Drag panning/orbiting, and Scroll zooming.

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

/// Pans (Right Click) or Orbits (Middle Click) the camera.
pub fn camera_pan_orbit(
    mut query: Query<(&mut Transform, &GlobalTransform), With<Camera3d>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    _keys: Res<ButtonInput<KeyCode>>, // Shift modifier for alternative?
    mouse_motion: Res<AccumulatedMouseMotion>,
    // select_tool_data: Option<Res<SelectToolData>>,
) {
    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    for (mut transform, _global) in query.iter_mut() {
        // Pan: Right Click
        if mouse_buttons.pressed(MouseButton::Right) {
            let sensitivity = transform.translation.z.abs() * 0.002;

            // Move along local X and Y axes? No, World XY plane usually.
            // But if rotated, local X/Y might be tilted.
            // Let's pan in World XY plane.
            // We need to move the camera such that the point under cursor stays fixed.
            // Simple approximation: move strictly in World X/Y.

            transform.translation.x -= delta.x * sensitivity;
            transform.translation.y += delta.y * sensitivity;
        }

        // Orbit: Middle Click
        if mouse_buttons.pressed(MouseButton::Middle) {
            let sensitivity = 0.005;

            // Orbit around a pivot point. Ideally the look-at point.
            // Assume look-at point is on Z=0 plane at center of screen?
            // Or just rotate around current position? No, rotate around target.
            // Camera setup: (0, -30, 60) looking at (0,0,0).

            // Simple orbit logic:
            // Rotate around Z axis (yaw) and local X axis (pitch).

            // Current Look Direction
            // We want to rotate around the focus point.
            // Finding focus point: intersection of forward vector with Z=0 plane?

            let forward = transform.forward();
            let origin = transform.translation;
            let normal = Vec3::Z;
            let denominator = forward.dot(normal);

            let pivot = if denominator.abs() > 0.0001 {
                let t = (0.0 - origin.z) / forward.z;
                origin + forward * t
            } else {
                Vec3::ZERO // Fallback
            };

            // Orbit calculation
            // Rotate transform around pivot

            let yaw = -delta.x * sensitivity;
            let pitch = -delta.y * sensitivity;

            // Rotate around pivot
            // 1. Translate to pivot relative
            let mut offset = transform.translation - pivot;

            // 2. Rotate
            // Yaw (around World Z)
            offset = Quat::from_rotation_z(yaw) * offset;
            transform.rotation = Quat::from_rotation_z(yaw) * transform.rotation;

            // Pitch (around Local X)
            let pitch_rot = Quat::from_rotation_x(pitch);
            offset = transform.rotation * pitch_rot * transform.rotation.inverse() * offset;
            transform.rotation = transform.rotation * pitch_rot;

            // 3. Translate back
            transform.translation = pivot + offset;
        }
    }
}

// Kept for backward compatibility if other modules reference it (like select_tool.rs)
// Or we update select_tool.rs.
// Let's update select_tool.rs instead to point to camera_pan_orbit.
// But for now, alias it?
pub use camera_pan_orbit as camera_pan;

fn camera_zoom(
    mut query: Query<&mut Transform, With<Camera3d>>,
    scroll_events: Res<AccumulatedMouseScroll>,
) {
    let scroll = scroll_events.delta.y;
    if scroll == 0.0 {
        return;
    }

    for mut transform in query.iter_mut() {
        // Move along forward vector to zoom towards cursor/center?
        // Simple Z-translation zoom (dolly) is easiest for top-down-ish view.
        // But if rotated, Z-translation might behave weirdly.
        // Better: Move along forward vector.

        let forward = transform.forward();
        // let _dist = transform.translation.length();

        // Approximate zoom scale
        // let sensitivity = 0.1;
        // let mut _factor = 1.0;

        // if scroll > 0.0 {
        //     _factor = 1.0 - sensitivity;
        // } else {
        //     _factor = 1.0 + sensitivity;
        // }

        // If we move along forward vector:
        let displacement = forward * (scroll * transform.translation.z.abs() * 0.1);
        let new_pos = transform.translation + displacement;

        // Check bounds (don't go through ground or too far)
        // Only constrain if we are looking somewhat down?
        // Z check is simple.
        if new_pos.z > 1.0 && new_pos.z < 5000.0 {
            transform.translation = new_pos;
        }
    }
}
