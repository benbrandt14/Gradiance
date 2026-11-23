//! Trackpad and touch camera gestures (brief S-21; feel row "Blender
//! (navigation)", `docs/interaction-feel.md`). Camera only: editing
//! gestures stay on the mouse.
//!
//! | Gesture | Camera |
//! |---|---|
//! | two fingers drag (touch screen) | pan |
//! | two fingers pinch (touch screen) | zoom about their midpoint |
//! | two fingers turn (touch screen) | roll the view about its axis |
//! | trackpad scroll (pixel units) | pan |
//! | `Ctrl` + trackpad scroll | zoom about the cursor |
//! | trackpad pinch (`PinchGesture`) | zoom about the cursor |
//! | trackpad rotate (`RotationGesture`) | roll the view |
//! | `PanGesture` | pan |
//!
//! **What each platform delivers** (winit, through Bevy 0.20's input
//! messages; read from the Bevy and winit docs, not tried on hardware):
//!
//! - **macOS:** `PinchGesture` and `RotationGesture` from the trackpad, and
//!   trackpad scroll in pixel units, so all three trackpad rows work. A
//!   mouse wheel scrolls in lines and still zooms.
//! - **Windows:** touch screens give `TouchInput`. A precision touchpad
//!   scrolls in lines, so its two-finger drag zooms like a wheel, and its
//!   pinch arrives as `Ctrl` + wheel, which zooms.
//! - **Linux:** touch screens give `TouchInput` (X11 and Wayland). Wayland
//!   touchpad scroll is in pixels (pan); X11 touchpad scroll is in lines
//!   (zoom). No pinch or rotate messages.
//! - **iOS:** `PanGesture`, `PinchGesture`, `RotationGesture` once enabled,
//!   plus touches.
//!
//! The rule that splits a wheel from a trackpad is the scroll unit: a
//! wheel scrolls in lines and zooms (`camera::pan_and_zoom_camera`), a
//! trackpad scrolls in pixels and pans here. Camera state is workstation
//! config: no command, no undo step.

use crate::PointerOverUi;
use crate::camera::{CameraRig, current_world_per_pixel, pan_by_pixels, zoom_about};
use bevy::input::gestures::{PanGesture, PinchGesture, RotationGesture};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Pixels of trackpad scroll that zoom as much as one wheel notch (with
/// `Ctrl`).
pub const SCROLL_PIXELS_PER_NOTCH: f32 = 40.0;

/// Two fingers' motion over one frame, in screen pixels (Y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoFinger {
    /// How far their midpoint moved.
    pub pan: Vec2,
    /// Their spread now over their spread before (above 1: apart, zoom in).
    pub spread: f32,
    /// How far the line between them turned, counter-clockwise as seen on
    /// screen, in radians.
    pub turn: f32,
    /// Their midpoint now.
    pub midpoint: Vec2,
}

/// The motion of two fingers from `before` to `now` (screen pixels). A
/// spread under a pixel before reads as no zoom and no turn. Pure.
#[must_use]
pub fn two_finger(before: [Vec2; 2], now: [Vec2; 2]) -> TwoFinger {
    let mid_before = (before[0] + before[1]) * 0.5;
    let midpoint = (now[0] + now[1]) * 0.5;
    // Y up, so a counter-clockwise turn on screen is positive.
    let up = |v: Vec2| Vec2::new(v.x, -v.y);
    let (a, b) = (up(before[1] - before[0]), up(now[1] - now[0]));
    let (spread, turn) = if a.length() < 1.0 || b.length() < 1.0 {
        (1.0, 0.0)
    } else {
        (b.length() / a.length(), a.perp_dot(b).atan2(a.dot(b)))
    };
    TwoFinger {
        pan: midpoint - mid_before,
        spread,
        turn,
        midpoint,
    }
}

/// The motion of one finger pair from `before` to `now` (id, position),
/// matched by id; `None` when the pair is not the same two fingers.
fn pair_motion(before: [(u64, Vec2); 2], now: [(u64, Vec2); 2]) -> Option<TwoFinger> {
    let at = |id: u64| before.iter().find(|(b, _)| *b == id).map(|(_, p)| *p);
    let first = at(now[0].0)?;
    let second = at(now[1].0)?;
    Some(two_finger([first, second], [now[0].1, now[1].1]))
}

/// Rolls the rig by `turn` radians so the scene turns with the fingers
/// (the camera turns the other way), and stops any orientation glide.
fn roll_with(rig: &mut CameraRig, turn: f32) {
    use std::f32::consts::{PI, TAU};
    if turn.abs() <= f32::EPSILON || !turn.is_finite() {
        return;
    }
    rig.target = None;
    rig.roll = (rig.roll - turn + PI).rem_euclid(TAU) - PI;
}

/// The gesture inputs of one frame.
#[derive(bevy::ecs::system::SystemParam)]
pub struct GestureInputs<'w, 's> {
    touches: Res<'w, Touches>,
    scroll: Res<'w, AccumulatedMouseScroll>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    settings: Res<'w, gradiance_domain::settings::CameraSettings>,
    pinch: MessageReader<'w, 's, PinchGesture>,
    rotation: MessageReader<'w, 's, RotationGesture>,
    pan: MessageReader<'w, 's, PanGesture>,
}

/// Drives the rig from touch and trackpad gestures (the table in the
/// module docs). Runs after `pan_and_zoom_camera`; off-UI only, and not
/// while a tool gesture holds the pointer.
pub fn touch_and_trackpad(
    mut rig: ResMut<CameraRig>,
    mut cameras: Query<(&mut Projection, &Camera, &GlobalTransform), With<Camera3d>>,
    mut input: GestureInputs,
    windows: Query<&Window, With<PrimaryWindow>>,
    over_ui: Res<PointerOverUi>,
    gesture: Res<crate::tools::ActiveGesture>,
    mut last_pair: Local<Option<[(u64, Vec2); 2]>>,
) {
    let pinch: f32 = input.pinch.read().map(|g| g.0).sum();
    let rotation: f32 = input.rotation.read().map(|g| g.0).sum();
    let pan_gesture: Vec2 = input.pan.read().map(|g| g.0).sum();
    let Ok((mut projection, camera, global)) = cameras.single_mut() else {
        return;
    };
    if over_ui.0 || gesture.0 {
        *last_pair = None;
        return;
    }
    let viewport_height = windows.iter().next().map_or(900.0, Window::height);
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let view = (camera, global);

    // The pair's positions last frame, by finger id. Bevy's
    // `previous_position` moves only on a frame with touch events, so
    // fingers at rest would repeat their last motion.
    let mut fingers = input.touches.iter();
    let pair = match (fingers.next(), fingers.next(), fingers.next()) {
        (Some(a), Some(b), None) => Some([(a.id(), a.position()), (b.id(), b.position())]),
        _ => None,
    };
    let before = std::mem::replace(&mut *last_pair, pair);
    if let (Some(now), Some(before)) = (pair, before)
        && let Some(motion) = pair_motion(before, now)
    {
        let wpp = current_world_per_pixel(&projection, &rig, viewport_height);
        pan_by_pixels(&mut rig, motion.pan, wpp);
        zoom_about(
            &mut rig,
            &mut projection,
            view,
            Some(motion.midpoint),
            (wpp, wpp / motion.spread),
            viewport_height,
        );
        roll_with(&mut rig, motion.turn);
    }

    let wpp = current_world_per_pixel(&projection, &rig, viewport_height);
    if input.scroll.unit == MouseScrollUnit::Pixel && input.scroll.delta != Vec2::ZERO {
        let ctrl =
            input.keys.pressed(KeyCode::ControlLeft) || input.keys.pressed(KeyCode::ControlRight);
        if ctrl {
            let step = input.settings.sanitized().zoom_step;
            let notches = input.scroll.delta.y / SCROLL_PIXELS_PER_NOTCH;
            zoom_about(
                &mut rig,
                &mut projection,
                view,
                cursor,
                (wpp, wpp * step.powf(notches)),
                viewport_height,
            );
        } else {
            pan_by_pixels(&mut rig, input.scroll.delta, wpp);
        }
    }
    let wpp = current_world_per_pixel(&projection, &rig, viewport_height);
    pan_by_pixels(&mut rig, pan_gesture, wpp);
    if pinch.is_finite() && pinch > -0.9 && pinch.abs() > f32::EPSILON {
        zoom_about(
            &mut rig,
            &mut projection,
            view,
            cursor,
            (wpp, wpp / (1.0 + pinch)),
            viewport_height,
        );
    }
    roll_with(&mut rig, rotation);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_fingers_moving_together_pan() {
        let m = two_finger(
            [Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0)],
            [Vec2::new(10.0, 5.0), Vec2::new(110.0, 5.0)],
        );
        assert_eq!(m.pan, Vec2::new(10.0, 5.0));
        assert!((m.spread - 1.0).abs() < 1e-6);
        assert!(m.turn.abs() < 1e-6);
    }

    #[test]
    fn two_fingers_apart_zoom_and_turning_rolls() {
        let m = two_finger(
            [Vec2::new(-50.0, 0.0), Vec2::new(50.0, 0.0)],
            [Vec2::new(-100.0, 0.0), Vec2::new(100.0, 0.0)],
        );
        assert!((m.spread - 2.0).abs() < 1e-6);
        assert_eq!(m.pan, Vec2::ZERO);
        // The right finger moves up the screen (Y down): counter-clockwise.
        let m = two_finger(
            [Vec2::new(-50.0, 0.0), Vec2::new(50.0, 0.0)],
            [Vec2::new(-50.0, 50.0), Vec2::new(50.0, -50.0)],
        );
        assert!((m.turn - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }

    #[test]
    fn fingers_on_one_spot_do_not_zoom() {
        let m = two_finger([Vec2::ZERO, Vec2::ZERO], [Vec2::ZERO, Vec2::X * 50.0]);
        assert!((m.spread - 1.0).abs() < 1e-6);
        assert_eq!(m.turn, 0.0);
    }
}
