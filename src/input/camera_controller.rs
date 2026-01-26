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

/// Handles Camera Pan (Middle Mouse) and Orbit (Right Mouse).
pub fn camera_pan_orbit(
    mut query: Query<(&mut Transform, &Projection), With<Camera>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    select_tool_data: Option<Res<SelectToolData>>,
) {
    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    // Check input state
    let is_pan = mouse_buttons.pressed(MouseButton::Middle);
    let is_orbit = mouse_buttons.pressed(MouseButton::Right);

    if !is_pan && !is_orbit {
        return;
    }

    // If rotating with Select Tool (Right Click), ignore Camera Orbit
    if is_orbit
        && let Some(data) = &select_tool_data
        && data.is_rotating
    {
        return;
    }

    for (mut transform, projection) in query.iter_mut() {
        if let Projection::Perspective(_) = projection {
            if is_pan {
                 // Pan: Move perpendicular to view direction
                 let right = transform.right();
                 let up = transform.up();
                 // Scale by distance for intuitiveness
                 let dist = transform.translation.z.abs().max(1.0);
                 let speed = dist * 0.002;

                 transform.translation -= right * delta.x * speed;
                 transform.translation += up * delta.y * speed;
            } else if is_orbit {
                 // Improved Orbit: Rotate around a ground point target.
                 // For now, assume focusing on (0,0,0) or projection.
                 // A simple tumble around the center of the screen at ground level.

                 // Let's assume the camera is looking at something at Z=0.
                 // Find the distance along the forward vector to Z=0 plane.
                 let normal = Vec3::Z;
                 let denom = transform.forward().dot(normal);
                 let t = if denom.abs() > 0.01 {
                     (Vec3::ZERO - transform.translation).dot(normal) / denom
                 } else {
                     30.0 // Fallback distance
                 };

                 let focus_point = transform.translation + transform.forward() * t;

                 let yaw = -delta.x * 0.005;
                 let pitch = -delta.y * 0.005;

                 // Rotate around focus point
                 // First Yaw (around global Z up in world, or Y up in standard 3D? Bevy uses Y up usually, but this is 2D physics on Z plane?)
                 // Physics uses XY plane. Z is depth. So Up is Y? No, "Gravity" is usually -Y.
                 // Visually, Bevy Camera LookAt (0,0,0) usually puts Y as Up.
                 // Wait, if we use standard Camera3d setup:
                 // Transform::from_xyz(0.0, -30.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y)
                 // This means Camera Up is roughly Y?
                 // Actually, LookingAt(Target, Up).
                 // Camera is at (0, -30, 60). Looking at (0,0,0). Up vector supplied is Y.

                 // If we orbit around Z axis (World Up for 2D plane logic?):
                 // No, standard 3D orbit usually orbits Y axis (Azimuth) and Local X axis (Elevation).

                 // Let's stick to standard 3D conventions:
                 // Rotate around Global Y (Yaw)
                 // Rotate around Local X (Pitch)

                 // Rotate around focus point
                 let mut new_transform = *transform;

                 // Yaw around focus point (Global Y)
                 let rot_yaw = Quat::from_axis_angle(Vec3::Y, yaw);
                 new_transform.rotate_around(focus_point, rot_yaw);

                 // Pitch around focus point (Local X computed from new transform)
                 let right = new_transform.right();
                 let rot_pitch = Quat::from_axis_angle(*right, pitch);
                 new_transform.rotate_around(focus_point, rot_pitch);

                 *transform = new_transform;
            }
        } else {
            // 2D Orthographic
            if is_pan || is_orbit { // Treat right click as pan in 2D too
                let zoom = transform.scale.x;
                transform.translation.x -= delta.x * zoom;
                transform.translation.y += delta.y * zoom;
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
