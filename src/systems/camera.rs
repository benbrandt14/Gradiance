use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};

#[derive(Component)]
pub struct PanCam {
    pub zoom_speed: f32,
    pub pan_speed: f32,
}

impl Default for PanCam {
    fn default() -> Self {
        Self {
            zoom_speed: 0.1,
            pan_speed: 1.0,
        }
    }
}

pub fn camera_movement(
    mut query: Query<(&mut Transform, &mut OrthographicProjection, &PanCam)>,
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
    mouse_button_input: Res<ButtonInput<MouseButton>>,
) {
    let mut delta = Vec2::ZERO;

    if mouse_button_input.pressed(MouseButton::Middle) {
        for event in mouse_motion_events.read() {
            delta += event.delta;
        }
    }

    let mut scroll = 0.0;
    for event in mouse_wheel_events.read() {
        scroll += event.y;
    }

    for (mut transform, mut projection, settings) in query.iter_mut() {
        if delta != Vec2::ZERO {
            transform.translation.x -= delta.x * settings.pan_speed * projection.scale;
            transform.translation.y += delta.y * settings.pan_speed * projection.scale;
        }

        if scroll != 0.0 {
             projection.scale -= scroll * settings.zoom_speed * projection.scale;
             projection.scale = projection.scale.clamp(0.1, 10.0);
        }
    }
}
