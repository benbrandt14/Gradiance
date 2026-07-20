//! Editor camera: a CAD-style rig.
//!
//! The camera is *derived* from [`CameraRig`] every frame: a 3D focus
//! point, an orbit (yaw/pitch/roll), and a distance. The orbit is
//! unrestricted (yaw wraps, pitch reaches the poles) and panning moves
//! the focus in the **view plane**, so the rig is a full 3D camera.
//! Editing normally happens in the straight-on 2D view; the view cube /
//! `Home` glide fluidly back to canonical orientations (re-centering the
//! focus depth onto the sandbox plane). Picking is **ray/plane**
//! (`cursor.rs`), so pointing stays exact at any tilt — the tilted view
//! is a first-class editing view.
//!
//! Projection is configurable (`ScenerySettings::perspective_deg`):
//! orthographic (0) for exact 2D work, or perspective for depth parallax
//! across the continuous layers. All screen↔world sizing goes through the
//! [`CameraScale`] resource, which both projections keep meaningful.
//!
//! Bindings: right-drag or arrows pan · wheel zooms at the cursor ·
//! middle-drag orbits (Shift+middle pans) · `Home` returns to 2D.

use crate::PointerOverUi;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use gradiance_domain::settings::ScenerySettings;

/// Keyboard pan speed in world pixels per second (at scale 1).
const KEY_PAN_SPEED: f32 = 600.0;
/// Zoom multiplier per scroll notch (closer to 1 = gentler).
const ZOOM_STEP: f32 = 0.94;
/// World-units-per-pixel zoom limits (both projections).
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 20.0;
/// Orbit sensitivity, radians per screen pixel.
const ORBIT_SPEED: f32 = 0.005;
/// Exponential rate of the glide toward a view target (per second).
const GLIDE_RATE: f32 = 8.0;

/// Half-range of the orthographic near/far slab: sized past the
/// infinite-plane quad corners (`PLANE_EXTENT`·√2 ≈ 707k) so the frustum
/// never cuts the megaquads as the view orbits — orthographic near/far
/// only define a depth range, so the large slab costs nothing.
pub const DEPTH_SLAB: f32 = 800_000.0;

/// The authoritative camera state; the `Transform` is derived from it.
#[derive(Resource, Debug, Clone, Copy)]
pub struct CameraRig {
    /// Look-at point (free in 3D; canonical-view glides pull `z` back
    /// onto the sandbox plane).
    pub focus: Vec3,
    /// Distance from the focus (perspective: the dolly axis; ortho: only
    /// affects clipping comfort).
    pub distance: f32,
    /// Orbit yaw about the world Y axis (radians; 0 = straight on).
    pub yaw: f32,
    /// Orbit pitch about the camera X axis (radians; 0 = straight on).
    pub pitch: f32,
    /// Roll about the view axis (radians; 0 = horizon level).
    pub roll: f32,
    /// While set, (yaw, pitch, roll) glide toward this target — the view
    /// cube and `Home` both steer through it.
    pub target: Option<Vec3>,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            distance: 600.0,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            target: None,
        }
    }
}

impl CameraRig {
    /// The rig's orientation.
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
            * Quat::from_rotation_x(self.pitch)
            * Quat::from_rotation_z(self.roll)
    }

    /// Whether the view is (essentially) the straight-on 2D view.
    pub fn is_flat(&self) -> bool {
        self.yaw.abs() < 1e-3 && self.pitch.abs() < 1e-3 && self.roll.abs() < 1e-3
    }

    /// Starts a glide back to the straight-on 2D view.
    pub fn glide_home(&mut self) {
        self.target = Some(Vec3::ZERO);
    }
}

/// World-units-per-logical-pixel of the editor camera at the sandbox
/// plane, refreshed every frame by [`apply_camera_rig`]. The single
/// screen↔world sizing authority — meaningful under both projections
/// (perspective uses the focus distance), so every glyph/pick radius
/// reads it instead of poking the projection.
#[derive(Resource, Debug, Clone, Copy)]
pub struct CameraScale(pub f32);

impl Default for CameraScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// The world-per-pixel factor for the current projection state.
fn world_per_pixel(projection: &Projection, distance: f32, viewport_height: f32) -> f32 {
    match projection {
        Projection::Orthographic(o) => o.scale,
        Projection::Perspective(p) => {
            2.0 * distance * (p.fov * 0.5).tan() / viewport_height.max(1.0)
        }
        Projection::Custom(_) => 1.0,
    }
}

/// Applies the configured projection kind (change-detected), preserving
/// the apparent zoom across the switch.
pub fn apply_projection(
    scenery: Res<ScenerySettings>,
    mut rig: ResMut<CameraRig>,
    mut cameras: Query<&mut Projection, With<Camera3d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(mut projection) = cameras.single_mut() else {
        return;
    };
    let viewport_height = windows.iter().next().map_or(900.0, Window::height);
    let fov_deg = scenery.perspective_deg.clamp(0.0, 120.0);
    let wpp = world_per_pixel(&projection, rig.distance, viewport_height);
    if fov_deg < 1.0 {
        if !matches!(*projection, Projection::Orthographic(_)) {
            *projection = Projection::Orthographic(OrthographicProjection {
                near: -DEPTH_SLAB,
                far: DEPTH_SLAB,
                scale: wpp,
                ..OrthographicProjection::default_3d()
            });
        }
    } else {
        let fov = fov_deg.to_radians();
        // Same world-per-pixel at the focus plane as before the switch.
        rig.distance = (wpp * viewport_height / (2.0 * (fov * 0.5).tan())).max(1.0);
        *projection = Projection::Perspective(PerspectiveProjection {
            fov,
            near: 1.0,
            far: 4.0 * DEPTH_SLAB,
            ..default()
        });
    }
}

/// Drives the rig from input: pan, zoom-at-cursor, orbit, and glides.
pub fn pan_and_zoom_camera(
    mut rig: ResMut<CameraRig>,
    mut cameras: Query<(&mut Projection, &Camera, &GlobalTransform), With<Camera3d>>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    over_ui: Res<PointerOverUi>,
    keyboard_captured: Res<crate::KeyboardCaptured>,
    gesture: Res<crate::tools::ActiveGesture>,
    time: Res<Time>,
) {
    let Ok((mut projection, camera, global)) = cameras.single_mut() else {
        return;
    };
    let viewport_height = windows.iter().next().map_or(900.0, Window::height);
    let wpp = world_per_pixel(&projection, rig.distance, viewport_height);

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
    let mut delta = pan * KEY_PAN_SPEED * time.delta_secs() * wpp;

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let middle = buttons.pressed(MouseButton::Middle);
    let orbiting = middle && !shift;
    let panning = buttons.pressed(MouseButton::Right) || (middle && shift);

    // Right-drag pans only when no tool gesture owns the pointer (the
    // select tool uses right-drag for rotation).
    if !over_ui.0 && !gesture.0 && panning {
        // Screen-space motion: X right, Y down → view X right, Y up.
        let m = motion.delta;
        delta += Vec2::new(-m.x, m.y) * wpp;
    }
    if delta != Vec2::ZERO {
        // Pan in the view plane: under the straight-on view this is the
        // sandbox XY; tilted, the camera strafes like any 3D editor.
        let pan3 = rig.rotation() * delta.extend(0.0);
        rig.focus += pan3;
    }

    // Middle-drag orbits (CAD inspect view); any orbit input cancels an
    // in-flight glide. The orbit is free: yaw wraps all the way around
    // and pitch reaches the poles (ray-plane picking is undefined only in
    // the measure-zero edge-on view, where tools simply don't pick).
    if !over_ui.0 && orbiting {
        let m = motion.delta;
        if m != Vec2::ZERO {
            use std::f32::consts::{FRAC_PI_2, PI, TAU};
            rig.target = None;
            rig.yaw = (rig.yaw + m.x * ORBIT_SPEED + PI).rem_euclid(TAU) - PI;
            rig.pitch = (rig.pitch - m.y * ORBIT_SPEED).clamp(-FRAC_PI_2, FRAC_PI_2);
        }
    }
    if keys.just_pressed(KeyCode::Home) && !keyboard_captured.0 {
        rig.glide_home();
    }
    if let Some(target) = rig.target {
        use std::f32::consts::{PI, TAU};
        let k = (-GLIDE_RATE * time.delta_secs()).exp();
        let mut current = Vec3::new(rig.yaw, rig.pitch, rig.roll);
        // Take the short way around the yaw circle (yaw wraps freely).
        if target.x - current.x > PI {
            current.x += TAU;
        } else if target.x - current.x < -PI {
            current.x -= TAU;
        }
        let next = target + (current - target) * k;
        rig.yaw = next.x;
        rig.pitch = next.y;
        rig.roll = next.z;
        // Canonical views re-center the focus depth onto the sandbox plane.
        rig.focus.z *= k;
        if (next - target).length() < 1e-3 && rig.focus.z.abs() < 1e-3 {
            rig.yaw = target.x;
            rig.pitch = target.y;
            rig.roll = target.z;
            rig.focus.z = 0.0;
            rig.target = None;
        }
    }

    // Zoom toward the cursor (plane-anchored, works at any tilt): ortho
    // scales the projection, perspective dollies the rig.
    let notches = scroll.delta.y;
    if notches.abs() > f32::EPSILON && !over_ui.0 {
        let factor = ZOOM_STEP.powf(notches);
        let new_wpp = (wpp * factor).clamp(MIN_SCALE, MAX_SCALE);
        if (new_wpp - wpp).abs() > f32::EPSILON {
            let anchor = windows
                .iter()
                .next()
                .and_then(Window::cursor_position)
                .and_then(|cursor| plane_point(camera, global, cursor));
            if let Some(anchor) = anchor {
                let ratio = new_wpp / wpp;
                let a = anchor.extend(rig.focus.z);
                rig.focus = a + (rig.focus - a) * ratio;
            }
            match &mut *projection {
                Projection::Orthographic(o) => o.scale = new_wpp,
                Projection::Perspective(p) => {
                    rig.distance = new_wpp * viewport_height / (2.0 * (p.fov * 0.5).tan());
                }
                Projection::Custom(_) => {}
            }
        }
    }
}

/// Derives the camera `Transform` from the rig and refreshes
/// [`CameraScale`] (runs after rig updates).
pub fn apply_camera_rig(
    rig: Res<CameraRig>,
    mut scale: ResMut<CameraScale>,
    mut cameras: Query<(&mut Transform, &Projection), With<Camera3d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok((mut transform, projection)) = cameras.single_mut() else {
        return;
    };
    let q = rig.rotation();
    let focus = rig.focus;
    *transform =
        Transform::from_translation(focus + q * Vec3::new(0.0, 0.0, rig.distance)).with_rotation(q);
    let viewport_height = windows.iter().next().map_or(900.0, Window::height);
    scale.0 = world_per_pixel(projection, rig.distance, viewport_height);
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
