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
    mut out: ResMut<CursorWorldPos>,
) {
    out.0 = (|| {
        let window = windows.iter().next()?;
        let cursor = window.cursor_position()?;
        let (camera, transform) = cameras.iter().next()?;
        camera.viewport_to_world_2d(transform, cursor).ok()
    })();
}
