use crate::commands::{GameCommand, SubmitGameCommand};
use crate::tools::ToolState;
use avian2d::prelude::*;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::sprite::Anchor;

pub struct PlaneToolPlugin;

impl Plugin for PlaneToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlaneToolState>();
        app.add_systems(Update, plane_tool_logic.run_if(in_state(ToolState::Plane)));
    }
}

#[derive(Resource, Default)]
struct PlaneToolState {
    start_pos: Option<Vec2>,
    current_pos: Option<Vec2>,
}

fn plane_tool_logic(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut state: ResMut<PlaneToolState>,
    mut gizmos: Gizmos,
) {
    let Ok((camera, camera_transform)) = camera_q.get_single() else {
        return;
    };
    let Ok(window) = windows.get_single() else {
        return;
    };

    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(point) = camera.viewport_to_world_2d(camera_transform, cursor_pos) {
            // Handle Input
            if mouse.just_pressed(MouseButton::Left) {
                state.start_pos = Some(point);
                state.current_pos = Some(point);
            }

            if mouse.pressed(MouseButton::Left) {
                state.current_pos = Some(point);
            }

            if mouse.just_released(MouseButton::Left) {
                if let (Some(start), Some(end)) = (state.start_pos, state.current_pos) {
                    if start.distance(end) > 5.0 {
                        let cmd = CreatePlaneCommand {
                            start,
                            end,
                            entity: None,
                        };
                        commands.queue(SubmitGameCommand(Box::new(cmd)));
                    }
                }
                state.start_pos = None;
                state.current_pos = None;
            }
        }
    }

    // Draw Preview
    if let (Some(start), Some(end)) = (state.start_pos, state.current_pos) {
        // Draw the surface line
        gizmos.line_2d(start, end, Color::WHITE);

        // Calculate Normal visual
        let diff = end - start;
        let center = start + diff / 2.0;
        // Normal is 90 deg CCW: (-y, x)
        let normal = Vec2::new(-diff.y, diff.x).normalize_or_zero() * 40.0;

        // Draw Normal
        gizmos.arrow_2d(center, center + normal, Color::srgb(1.0, 1.0, 0.0));
    }
}

struct CreatePlaneCommand {
    start: Vec2,
    end: Vec2,
    entity: Option<Entity>,
}

impl GameCommand for CreatePlaneCommand {
    fn execute(&mut self, world: &mut World) {
        let diff = self.end - self.start;
        let angle = diff.y.atan2(diff.x);
        let center = self.start + diff / 2.0;

        let id = world
            .spawn((
                RigidBody::Static,
                Collider::half_space(Vec2::Y),
                Friction::default(),
                Restitution::new(0.5),
                Transform {
                    translation: center.extend(0.0),
                    rotation: Quat::from_rotation_z(angle),
                    ..default()
                },
                Sprite {
                    color: Color::WHITE,
                    custom_size: Some(Vec2::new(100000.0, 100000.0)),
                    anchor: Anchor::TopCenter,
                    ..default()
                },
            ))
            .id();
        self.entity = Some(id);
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(e) = self.entity {
            world.despawn(e);
        }
    }
}
