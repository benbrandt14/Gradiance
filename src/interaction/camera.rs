//! Editor camera: pan (right/middle drag, arrow keys) and zoom-at-cursor.

use crate::interaction::PointerOverUi;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Keyboard pan speed in world pixels per second (at scale 1).
const KEY_PAN_SPEED: f32 = 600.0;
/// Zoom multiplier per scroll notch.
const ZOOM_STEP: f32 = 0.9;
/// Orthographic scale limits.
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 20.0;

/// Pans with right/middle mouse drag or arrow keys; zooms toward the
/// cursor with the scroll wheel (Algodoo behavior: the point under the
/// pointer stays put while zooming).
pub fn pan_and_zoom_camera(
    mut cameras: Query<
        (&mut Transform, &mut Projection, &Camera, &GlobalTransform),
        With<Camera3d>,
    >,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    over_ui: Res<PointerOverUi>,
    gesture: Res<crate::interaction::tools::ActiveGesture>,
    time: Res<Time>,
) {
    let Ok((mut transform, mut projection, camera, global)) = cameras.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = &mut *projection else {
        return;
    };

    // Keyboard pan always works; pointer gestures only off-UI.
    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowLeft) {
        pan.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        pan.x += 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        pan.y -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowUp) {
        pan.y += 1.0;
    }
    let mut delta = pan * KEY_PAN_SPEED * time.delta_secs() * ortho.scale;

    // Right-drag pans only when no tool gesture owns the pointer (the
    // select tool uses right-drag for rotation).
    if !over_ui.0
        && !gesture.0
        && (buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle))
    {
        // Screen-space motion: X right, Y down → world X right, Y up.
        let m = motion.delta;
        delta += Vec2::new(-m.x, m.y) * ortho.scale;
    }
    transform.translation += delta.extend(0.0);

    // Zoom toward the cursor.
    let notches = scroll.delta.y;
    if notches.abs() > f32::EPSILON && !over_ui.0 {
        let old_scale = ortho.scale;
        let new_scale = (old_scale * ZOOM_STEP.powf(notches)).clamp(MIN_SCALE, MAX_SCALE);
        if (new_scale - old_scale).abs() > f32::EPSILON {
            let anchor = windows
                .iter()
                .next()
                .and_then(Window::cursor_position)
                .and_then(|cursor| camera.viewport_to_world_2d(global, cursor).ok());
            if let Some(anchor) = anchor {
                let cam_pos = transform.translation.truncate();
                let ratio = new_scale / old_scale;
                let new_pos = anchor + (cam_pos - anchor) * ratio;
                transform.translation.x = new_pos.x;
                transform.translation.y = new_pos.y;
            }
            ortho.scale = new_scale;
        }
    }
}

/// The current world-units-per-screen-pixel factor of the editor camera.
pub fn camera_scale(cameras: &Query<&Projection, With<Camera3d>>) -> f32 {
    cameras
        .iter()
        .next()
        .and_then(|p| match p {
            Projection::Orthographic(o) => Some(o.scale),
            _ => None,
        })
        .unwrap_or(1.0)
}
