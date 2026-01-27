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
        // Constraint: Pitch (local X) and Yaw (GLOBAL Y). No Roll (local Z).
        if mouse_buttons.pressed(MouseButton::Middle) {
            let sensitivity = 0.005;

            // Find pivot point (intersection with Z=0 plane)
            let forward = transform.forward();
            let origin = transform.translation;
            // Plane normal is Z (0,0,1).
            // But wait, if we want "Up" to be Y (physics world Up),
            // then we are viewing a "Wall".
            // Camera setup: (0, -30, 500) looking at (0,0,0).
            // Up is Y. Z is Depth.
            // So we orbit around Y axis (Yaw) and Local X (Pitch).

            // Intersection with Z=0 plane still makes sense as focus point.
            let normal = Vec3::Z;
            let denominator = forward.dot(normal);

            let pivot = if denominator.abs() > 0.0001 {
                let t = (0.0 - origin.z) / forward.z;
                origin + forward * t
            } else {
                Vec3::ZERO // Fallback
            };

            let yaw = -delta.x * sensitivity;
            let pitch = -delta.y * sensitivity;

            // 1. Translate to pivot relative
            let offset = transform.translation - pivot;

            // 2. Rotate
            // Yaw around GLOBAL Y (0,1,0)?
            // Wait, if 2D plane is XY, then Y is Up.
            // So Yaw around Y.
            // Pitch around Local Right (X).

            let mut rotation = transform.rotation;

            // Apply Pitch (Local X)
            let pitch_rot = Quat::from_rotation_x(pitch);
            rotation = rotation * pitch_rot;

            // Apply Yaw (Global Y)? Or Global Z?
            // If we are looking at XY plane, and Y is Up.
            // Then rotating around Y (0,1,0) moves the camera left/right around the vertical axis.
            // This assumes Y is vertical.
            // Let's try Yaw around Y.
            // But previous setup was Yaw around Z (Roll?).
            // If camera is at (0, -30, 500), looking at origin.
            // It's mostly looking along -Z.
            // Y is Up on screen. X is Right.
            // Rotating around Z spins the view (Roll).
            // Rotating around Y orbits horizontally (Yaw).
            // Rotating around X orbits vertically (Pitch).

            // User said "should not roll". So no Z rotation.
            // Only Pitch (X) and Yaw (Y).

            // Yaw (Global Y)
            let yaw_rot = Quat::from_rotation_y(yaw);
            // Apply Yaw to position: rotate offset around Y.
            let mut new_offset = yaw_rot * offset;
            // Apply Pitch to position: rotate offset around camera's local X?
            // This is tricky. Standard orbit: rotate position around pivot.
            // We need to accumulate rotations on the transform.

            // Let's use `look_at`.
            // Calculate new position on sphere.
            // Current position relative to pivot: `offset`.
            // Convert to spherical coords? Or just rotate vector.

            // Rotate offset by Yaw (Global Y)
            new_offset = Quat::from_rotation_y(yaw) * new_offset;

            // Rotate offset by Pitch (Local Right).
            // Local Right depends on current rotation.
            // But we want to prevent roll.
            // We can calculate right vector from Cross(Forward, WorldUp).
            // WorldUp = Y (0,1,0).
            let fwd = -new_offset.normalize(); // Looking at pivot
            let right = fwd.cross(Vec3::Y).normalize_or_zero();
            // If right is zero (looking straight up/down), handle singularity?
            if right.length_squared() < 0.001 {
               // Singularity. Skip pitch.
            } else {
               let pitch_rot_axis = Quat::from_axis_angle(right, pitch);
               new_offset = pitch_rot_axis * new_offset;
            }

            transform.translation = pivot + new_offset;
            transform.look_at(pivot, Vec3::Y); // Enforce Up=Y to prevent roll
        }
    }
}

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
        let forward = transform.forward();

        let displacement = forward * (scroll * transform.translation.z.abs() * 0.1);
        let new_pos = transform.translation + displacement;

        if new_pos.z > 1.0 && new_pos.z < 5000.0 {
            transform.translation = new_pos;
        }
    }
}
