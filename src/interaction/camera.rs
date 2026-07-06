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

/// Saved straight-on camera pose while the depth peek (Tab) is held.
///
/// The peek tilts the camera about its ground focus so the 2.5D
/// extrusion and layer depth become visible; it is **view-only** — the
/// cursor goes inert while peeking (see `cursor::update_cursor_world_pos`)
/// so no tool ever acts on a tilted projection.
#[derive(Resource, Default, Debug)]
pub struct DepthPeek {
    saved: Option<Transform>,
}

impl DepthPeek {
    /// Whether the peek view is currently active.
    pub fn active(&self) -> bool {
        self.saved.is_some()
    }
}

/// Holds Tab to orbit the camera down by `DebugSettings::peek_tilt_deg`,
/// releasing snaps back to the straight-on editing view.
pub fn depth_peek(
    keys: Res<ButtonInput<KeyCode>>,
    keyboard_captured: Res<crate::interaction::KeyboardCaptured>,
    debug: Res<crate::domain::settings::DebugSettings>,
    mut peek: ResMut<DepthPeek>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(mut transform) = cameras.single_mut() else {
        return;
    };
    if keys.pressed(KeyCode::Tab) && !keyboard_captured.0 {
        if peek.saved.is_none() {
            peek.saved = Some(*transform);
        }
        if let Some(base) = peek.saved {
            let focus = Vec3::new(base.translation.x, base.translation.y, 0.0);
            let distance = base.translation.z.max(1.0);
            let q = Quat::from_rotation_x(-debug.peek_tilt_deg.to_radians());
            *transform = Transform::from_translation(focus + q * Vec3::new(0.0, 0.0, distance))
                .with_rotation(q);
        }
    } else if let Some(saved) = peek.saved.take() {
        *transform = saved;
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
