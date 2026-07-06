//! Editor camera: a CAD-style rig.
//!
//! The camera is *derived* from [`CameraRig`] every frame: a focus point
//! on the sandbox plane, an orbit (yaw/pitch), and a distance. Editing
//! normally happens in the straight-on 2D view; middle-drag orbits to
//! inspect the 2.5D extrusion, `Home` (or double-tap) glides fluidly
//! back to 2D. Picking is **ray/plane** (`cursor.rs`), so pointing stays
//! exact at any tilt — the tilted view is a first-class editing view.
//!
//! Bindings: right-drag or arrows pan · wheel zooms at the cursor ·
//! middle-drag orbits (Shift+middle pans) · `Home` returns to 2D.

use crate::interaction::PointerOverUi;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Keyboard pan speed in world pixels per second (at scale 1).
const KEY_PAN_SPEED: f32 = 600.0;
/// Zoom multiplier per scroll notch (closer to 1 = gentler).
const ZOOM_STEP: f32 = 0.94;
/// Orthographic scale limits.
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 20.0;
/// Orbit sensitivity, radians per screen pixel.
const ORBIT_SPEED: f32 = 0.005;
/// Pitch/yaw limits (never see behind the backdrop).
const MAX_TILT: f32 = 1.35;
/// Exponential rate of the glide back to the 2D view (per second).
const HOME_RATE: f32 = 8.0;

/// The authoritative camera state; the `Transform` is derived from it.
#[derive(Resource, Debug, Clone, Copy)]
pub struct CameraRig {
    /// Look-at point on the sandbox plane.
    pub focus: Vec2,
    /// Distance from the focus (ortho: only affects clipping comfort).
    pub distance: f32,
    /// Orbit yaw about the world Y axis (radians; 0 = straight on).
    pub yaw: f32,
    /// Orbit pitch about the camera X axis (radians; 0 = straight on).
    pub pitch: f32,
    /// While set, yaw/pitch glide back to zero (the 2D view).
    pub homing: bool,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec2::ZERO,
            distance: 600.0,
            yaw: 0.0,
            pitch: 0.0,
            homing: false,
        }
    }
}

impl CameraRig {
    /// The rig's orientation.
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch)
    }

    /// Whether the view is (essentially) the straight-on 2D view.
    pub fn is_flat(&self) -> bool {
        self.yaw.abs() < 1e-3 && self.pitch.abs() < 1e-3
    }
}

/// Drives the rig from input: pan, zoom-at-cursor, orbit, and homing.
pub fn pan_and_zoom_camera(
    mut rig: ResMut<CameraRig>,
    mut cameras: Query<(&mut Projection, &Camera, &GlobalTransform), With<Camera3d>>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    over_ui: Res<PointerOverUi>,
    keyboard_captured: Res<crate::interaction::KeyboardCaptured>,
    gesture: Res<crate::interaction::tools::ActiveGesture>,
    time: Res<Time>,
) {
    let Ok((mut projection, camera, global)) = cameras.single_mut() else {
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

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let middle = buttons.pressed(MouseButton::Middle);
    let orbiting = middle && !shift;
    let panning = buttons.pressed(MouseButton::Right) || (middle && shift);

    // Right-drag pans only when no tool gesture owns the pointer (the
    // select tool uses right-drag for rotation).
    if !over_ui.0 && !gesture.0 && panning {
        // Screen-space motion: X right, Y down → world X right, Y up.
        let m = motion.delta;
        delta += Vec2::new(-m.x, m.y) * ortho.scale;
    }
    rig.focus += delta;

    // Middle-drag orbits (CAD inspect view); any orbit input cancels an
    // in-flight homing glide.
    if !over_ui.0 && orbiting {
        let m = motion.delta;
        if m != Vec2::ZERO {
            rig.homing = false;
            rig.yaw = (rig.yaw + m.x * ORBIT_SPEED).clamp(-MAX_TILT, MAX_TILT);
            rig.pitch = (rig.pitch - m.y * ORBIT_SPEED).clamp(-MAX_TILT, MAX_TILT);
        }
    }
    if keys.just_pressed(KeyCode::Home) && !keyboard_captured.0 {
        rig.homing = true;
    }
    if rig.homing {
        let k = (-HOME_RATE * time.delta_secs()).exp();
        rig.yaw *= k;
        rig.pitch *= k;
        if rig.yaw.abs() < 1e-3 && rig.pitch.abs() < 1e-3 {
            rig.yaw = 0.0;
            rig.pitch = 0.0;
            rig.homing = false;
        }
    }

    // Zoom toward the cursor (plane-anchored, works at any tilt).
    let notches = scroll.delta.y;
    if notches.abs() > f32::EPSILON && !over_ui.0 {
        let old_scale = ortho.scale;
        let new_scale = (old_scale * ZOOM_STEP.powf(notches)).clamp(MIN_SCALE, MAX_SCALE);
        if (new_scale - old_scale).abs() > f32::EPSILON {
            let anchor = windows
                .iter()
                .next()
                .and_then(Window::cursor_position)
                .and_then(|cursor| plane_point(camera, global, cursor));
            if let Some(anchor) = anchor {
                let ratio = new_scale / old_scale;
                rig.focus = anchor + (rig.focus - anchor) * ratio;
            }
            ortho.scale = new_scale;
        }
    }
}

/// Derives the camera `Transform` from the rig (runs after rig updates).
pub fn apply_camera_rig(rig: Res<CameraRig>, mut cameras: Query<&mut Transform, With<Camera3d>>) {
    let Ok(mut transform) = cameras.single_mut() else {
        return;
    };
    let q = rig.rotation();
    let focus = rig.focus.extend(0.0);
    *transform =
        Transform::from_translation(focus + q * Vec3::new(0.0, 0.0, rig.distance)).with_rotation(q);
}

/// Intersects the camera ray through `screen` with the sandbox plane
/// (z = 0). Exact for any camera orientation — this is what makes the
/// tilted view a real editing view.
pub fn plane_point(camera: &Camera, global: &GlobalTransform, screen: Vec2) -> Option<Vec2> {
    let ray = camera.viewport_to_world(global, screen).ok()?;
    let denominator = ray.direction.z;
    if denominator.abs() < 1e-6 {
        return None;
    }
    let t = -ray.origin.z / denominator;
    if t < 0.0 {
        return None;
    }
    Some((ray.origin + *ray.direction * t).truncate())
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
