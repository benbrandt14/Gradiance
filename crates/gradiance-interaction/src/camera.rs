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
//! middle-drag orbits (Shift+middle pans) · `Home` returns to 2D · `.`
//! frames the selection.
//!
//! Follow and frame (brief S-9): a [`CameraRequest`] (from the
//! `camera.follow` / `camera.frame` / `camera.look` operations) makes the
//! rig follow a body, or glide its focus and zoom to a view. A pan by hand
//! stops a follow. Camera state is workstation config: never undone, never
//! saved with the scene.

use crate::PointerOverUi;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::settings::{CameraSettings, ScenerySettings};

// Zoom step, keyboard pan speed and orbit sensitivity are the person's,
// not constants: `CameraSettings` (workstation config).

/// World-units-per-pixel zoom limits (both projections).
const MIN_SCALE: f32 = 0.0005;
const MAX_SCALE: f32 = 0.2;
/// Exponential rate of the glide toward a view target (per second).
const GLIDE_RATE: f32 = 8.0;

/// Half-range of the orthographic near/far slab: sized past the
/// infinite-plane quad corners (`PLANE_EXTENT`·√2 ≈ 707k) so the frustum
/// never cuts the megaquads as the view orbits — orthographic near/far
/// only define a depth range, so the large slab costs nothing.
pub const DEPTH_SLAB: f32 = 8_000.0;

/// Perspective near-clip distance (world metres) and the matching floor on the
/// rig distance. This is ~1 screen pixel at a deep zoom — it was literally
/// `1.0` *pixel* before the SI flip and the flip missed it, leaving a **1 m**
/// near plane: in perspective mode (`perspective_deg > 0`) the camera could not
/// approach within a metre and anything nearer was clipped away.
const PERSPECTIVE_NEAR: f32 = 0.01;

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
    /// The body the focus follows each frame (brief S-9), until a pan by
    /// hand, another view request or the body's removal stops it.
    pub follow: Option<StableId>,
    /// While set, the focus and the zoom glide toward this view (frame,
    /// look).
    pub view: Option<ViewTarget>,
}

/// A view the rig glides to: a focus on the sandbox plane and a zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTarget {
    /// The focus, in metres.
    pub focus: Vec2,
    /// World metres per screen pixel; `None` keeps the zoom.
    pub world_per_pixel: Option<f32>,
}

/// A camera request from an operation (brief S-9). Editor state: no
/// command, no undo step.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum CameraRequest {
    /// Follow this body; `None` stops following.
    Follow(Option<StableId>),
    /// Glide to show this world box `(min, max)` with a margin.
    Frame(Vec2, Vec2),
    /// Glide to this view.
    Look(ViewTarget),
}

/// The share of the viewport a framed box leaves free on each side.
pub const FRAME_MARGIN: f32 = 0.1;

/// The view that shows the world box `(min, max)` in a viewport of
/// `viewport` pixels, with [`FRAME_MARGIN`] free on each side. A box
/// smaller than a metre frames as a metre, so a point does not zoom in
/// past use.
pub fn frame_view(min: Vec2, max: Vec2, viewport: Vec2) -> ViewTarget {
    let size = (max - min).max(Vec2::ONE);
    let usable = (viewport * (1.0 - 2.0 * FRAME_MARGIN)).max(Vec2::ONE);
    let wpp = (size / usable).max_element().clamp(MIN_SCALE, MAX_SCALE);
    ViewTarget {
        focus: (min + max) * 0.5,
        world_per_pixel: Some(wpp),
    }
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            distance: 6.0,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            target: None,
            follow: None,
            view: None,
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
        rig.distance = (wpp * viewport_height / (2.0 * (fov * 0.5).tan())).max(PERSPECTIVE_NEAR);
        *projection = Projection::Perspective(PerspectiveProjection {
            fov,
            near: PERSPECTIVE_NEAR,
            far: 4.0 * DEPTH_SLAB,
            ..default()
        });
    }
}

/// Advances the right-button pan's engagement for one frame: records the
/// press, forgets it on release, and engages only once the cursor has
/// left the shared right-click deadzone — a release inside the deadzone
/// belongs to the context menu, so the pan must not have eaten it. Pure
/// over its arguments so the rule is unit-testable without a rig.
fn right_pan_engaged(
    buttons: &ButtonInput<MouseButton>,
    cursor: Option<Vec2>,
    press: &mut Option<Vec2>,
    engaged: &mut bool,
) -> bool {
    if buttons.just_pressed(MouseButton::Right) {
        *press = cursor;
        *engaged = false;
    }
    if !buttons.pressed(MouseButton::Right) {
        *press = None;
    } else if let (Some(start), Some(now)) = (*press, cursor)
        && !*engaged
        && start.distance(now) > crate::pointer::RIGHT_CLICK_DEADZONE_PX
    {
        *engaged = true;
    }
    buttons.pressed(MouseButton::Right) && *engaged
}

/// Drives the rig from input: pan, zoom-at-cursor, orbit, and glides.
pub fn pan_and_zoom_camera(
    mut rig: ResMut<CameraRig>,
    mut cameras: Query<(&mut Projection, &Camera, &GlobalTransform), With<Camera3d>>,
    settings: Res<CameraSettings>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: crate::graph_keys::EditorKeys,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    over_ui: Res<PointerOverUi>,
    keyboard_captured: Res<crate::KeyboardCaptured>,
    gesture: Res<crate::tools::ActiveGesture>,
    time: Res<Time>,
    // The right press that may become a pan, and whether it has left the
    // right-click deadzone yet: a release inside the deadzone belongs to
    // the context menu, so panning starts only past it, like rotate.
    mut right_press: Local<Option<Vec2>>,
    mut right_engaged: Local<bool>,
) {
    let Ok((mut projection, camera, global)) = cameras.single_mut() else {
        return;
    };
    let viewport_height = windows.iter().next().map_or(900.0, Window::height);
    let wpp = world_per_pixel(&projection, rig.distance, viewport_height);
    let feel = settings.sanitized();
    let orbit_speed = feel.orbit_speed_deg.to_radians();

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
    // Screen pixels per second × world per pixel: the same apparent speed
    // at every zoom.
    let mut delta = pan * feel.pan_speed * time.delta_secs() * wpp;

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let middle = buttons.pressed(MouseButton::Middle);
    let orbiting = middle && !shift;

    // The right-button pan claims its drag only past the right-click
    // deadzone (the same one the context menu and the rotate gesture
    // use), so a sloppy right-click neither pans nor rotates: the menu
    // owns it. Middle-drag keeps its immediate feel.
    let cursor_now = windows.iter().next().and_then(Window::cursor_position);
    let right_panning =
        right_pan_engaged(&buttons, cursor_now, &mut right_press, &mut right_engaged);
    let panning = right_panning || (middle && shift);

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
        // A pan by hand takes the camera back.
        rig.follow = None;
        rig.view = None;
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
            rig.yaw = (rig.yaw + m.x * orbit_speed + PI).rem_euclid(TAU) - PI;
            rig.pitch = (rig.pitch - m.y * orbit_speed).clamp(-FRAC_PI_2, FRAC_PI_2);
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
        let factor = feel.zoom_step.powf(notches);
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

/// The size of the primary window in logical pixels, or 1600 x 900 when
/// there is none (headless).
fn viewport_size(windows: &Query<&Window, With<PrimaryWindow>>) -> Vec2 {
    windows.iter().next().map_or(Vec2::new(1600.0, 900.0), |w| {
        Vec2::new(w.width(), w.height())
    })
}

/// Answers [`CameraRequest`]s: a follow replaces any glide, a frame or a
/// look replaces any follow.
pub fn apply_camera_requests(
    mut requests: MessageReader<CameraRequest>,
    mut rig: ResMut<CameraRig>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    for request in requests.read() {
        match *request {
            CameraRequest::Follow(body) => {
                rig.follow = body;
                rig.view = None;
            }
            CameraRequest::Frame(min, max) => {
                rig.follow = None;
                rig.view = Some(frame_view(min, max, viewport_size(&windows)));
            }
            CameraRequest::Look(view) => {
                rig.follow = None;
                rig.view = Some(ViewTarget {
                    world_per_pixel: view.world_per_pixel.map(|w| w.clamp(MIN_SCALE, MAX_SCALE)),
                    ..view
                });
            }
        }
    }
}

/// Moves the focus onto the followed body (its pose after this frame's
/// physics), and glides toward a requested view.
pub fn follow_and_glide(
    mut rig: ResMut<CameraRig>,
    index: Res<IdIndex>,
    bodies: Query<&Transform, With<gradiance_domain::Body>>,
    mut cameras: Query<&mut Projection, With<Camera3d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
) {
    if let Some(id) = rig.follow {
        match index.entity(id).and_then(|e| bodies.get(e).ok()) {
            Some(transform) => {
                rig.focus.x = transform.translation.x;
                rig.focus.y = transform.translation.y;
            }
            None => rig.follow = None,
        }
    }
    let Some(view) = rig.view else {
        return;
    };
    let k = (-GLIDE_RATE * time.delta_secs()).exp();
    let focus = view.focus + (rig.focus.truncate() - view.focus) * k;
    rig.focus.x = focus.x;
    rig.focus.y = focus.y;
    let viewport_height = viewport_size(&windows).y;
    let mut zoom_done = true;
    if let (Some(goal), Ok(mut projection)) = (view.world_per_pixel, cameras.single_mut()) {
        let wpp = world_per_pixel(&projection, rig.distance, viewport_height);
        // Glide the zoom in log space: equal steps look equal.
        let next = (goal.ln() + (wpp.ln() - goal.ln()) * k).exp();
        let next = if (next / goal - 1.0).abs() < 1e-3 {
            goal
        } else {
            zoom_done = false;
            next
        };
        match &mut *projection {
            Projection::Orthographic(o) => o.scale = next,
            Projection::Perspective(p) => {
                rig.distance =
                    (next * viewport_height / (2.0 * (p.fov * 0.5).tan())).max(PERSPECTIVE_NEAR);
            }
            Projection::Custom(_) => {}
        }
    }
    if zoom_done && focus.distance(view.focus) < 1e-4 {
        rig.focus.x = view.focus.x;
        rig.focus.y = view.focus.y;
        rig.view = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_framed_box_fits_with_its_margin() {
        let view = frame_view(
            Vec2::new(-4.0, 0.0),
            Vec2::new(6.0, 2.0),
            Vec2::new(1600.0, 900.0),
        );
        assert_eq!(view.focus, Vec2::new(1.0, 1.0));
        let wpp = view.world_per_pixel.unwrap_or_default();
        // Width decides: 10 m over 80 % of 1600 px.
        assert!((wpp - 10.0 / 1280.0).abs() < 1e-6, "{wpp}");
        // A point frames as one metre.
        let point = frame_view(Vec2::ONE, Vec2::ONE, Vec2::new(1600.0, 900.0));
        assert!((point.world_per_pixel.unwrap_or_default() - 1.0 / 720.0).abs() < 1e-6);
    }

    /// A right press-and-release inside the deadzone never engages the pan
    /// (the context menu owns that click); motion past it engages, and the
    /// pan stays owned to the release. `ButtonInput::clear` between calls
    /// is what `InputSystems` does at the top of every frame.
    #[test]
    fn right_pan_engages_only_past_the_click_deadzone() {
        let mut buttons = ButtonInput::<MouseButton>::default();
        let mut press = None;
        let mut engaged = false;
        let at = |x: f32| Some(Vec2::new(x, 0.0));

        buttons.press(MouseButton::Right);
        assert!(!right_pan_engaged(
            &buttons,
            at(0.0),
            &mut press,
            &mut engaged
        ));

        let inside = crate::pointer::RIGHT_CLICK_DEADZONE_PX - 1.0;
        buttons.clear();
        assert!(
            !right_pan_engaged(&buttons, at(inside), &mut press, &mut engaged),
            "a click inside the deadzone is the menu's, not the pan's"
        );

        let past = crate::pointer::RIGHT_CLICK_DEADZONE_PX + 1.0;
        buttons.clear();
        assert!(right_pan_engaged(
            &buttons,
            at(past),
            &mut press,
            &mut engaged
        ));

        buttons.clear();
        assert!(
            right_pan_engaged(&buttons, at(inside), &mut press, &mut engaged),
            "once engaged the pan keeps the drag, back inside the deadzone too"
        );

        buttons.release(MouseButton::Right);
        buttons.clear();
        assert!(!right_pan_engaged(
            &buttons,
            at(past),
            &mut press,
            &mut engaged
        ));
        assert!(press.is_none(), "the release forgets the press");
    }
}
