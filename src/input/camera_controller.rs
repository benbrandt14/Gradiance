//! Camera controller (pan/zoom).

use crate::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};

/// Plugin that handles camera movement (pan/zoom).
pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, camera_controller);
    }
}

fn camera_controller(
    mut query: Query<(&mut Transform, &mut OrthographicProjection), With<Camera2d>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion_evr: EventReader<MouseMotion>,
    mut scroll_evr: EventReader<MouseWheel>,
    _time: Res<Time>,
) {
    let (mut transform, mut projection) = match query.get_single_mut() {
        Ok(q) => q,
        Err(_) => return,
    };

    // Pan
    if mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle) {
        let mut delta = Vec2::ZERO;
        for ev in motion_evr.read() {
            delta += ev.delta;
        }

        // Scale pan speed by zoom
        transform.translation.x -= delta.x * projection.scale;
        transform.translation.y += delta.y * projection.scale;
    }

    // Zoom
    for ev in scroll_evr.read() {
        // Logarithmic zoom
        let zoom_speed = 0.1;
        let zoom_factor = 1.0 - ev.y * zoom_speed;
        projection.scale *= zoom_factor;
        projection.scale = projection.scale.clamp(0.1, 100.0);
    }
}
