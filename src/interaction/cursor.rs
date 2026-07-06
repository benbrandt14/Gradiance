//! World-space cursor position.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// The pointer's position on the sandbox plane, if a window and camera
/// exist and the pointer is inside the window.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorldPos(pub Option<Vec2>);

/// Updates [`CursorWorldPos`] from the primary window + editor camera.
pub fn update_cursor_world_pos(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    peek: Res<crate::interaction::camera::DepthPeek>,
    mut out: ResMut<CursorWorldPos>,
) {
    // Headless (no window): leave the resource alone so tests can inject
    // cursor positions directly.
    let Some(window) = windows.iter().next() else {
        return;
    };
    // Depth peek is view-only: with the camera tilted, the flat 2D
    // projection is meaningless, so tools see "no cursor".
    if peek.active() {
        out.0 = None;
        return;
    }
    out.0 = (|| {
        let cursor = window.cursor_position()?;
        let (camera, transform) = cameras.iter().next()?;
        camera.viewport_to_world_2d(transform, cursor).ok()
    })();
}
