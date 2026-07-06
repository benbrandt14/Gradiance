//! World-space cursor position.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// The pointer's position on the sandbox plane, if a window and camera
/// exist and the pointer is inside the window.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorldPos(pub Option<Vec2>);

/// Updates [`CursorWorldPos`] by intersecting the camera ray with the
/// sandbox plane — exact under any camera orbit, not just straight-on.
pub fn update_cursor_world_pos(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut out: ResMut<CursorWorldPos>,
) {
    // Headless (no window): leave the resource alone so tests can inject
    // cursor positions directly.
    let Some(window) = windows.iter().next() else {
        return;
    };
    out.0 = (|| {
        let cursor = window.cursor_position()?;
        let (camera, transform) = cameras.iter().next()?;
        crate::interaction::camera::plane_point(camera, transform, cursor)
    })();
}
