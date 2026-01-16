use crate::commands::{GameCommand, SubmitGameCommand};
use crate::tools::ToolState;
use avian2d::prelude::*;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;

pub struct CircleToolPlugin;

impl Plugin for CircleToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CircleToolState>();
        app.add_systems(
            Update,
            circle_tool_logic.run_if(in_state(ToolState::Circle)),
        );
    }
}

#[derive(Resource, Default)]
struct CircleToolState {
    start_pos: Option<Vec2>,
    current_pos: Option<Vec2>,
}

fn circle_tool_logic(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut state: ResMut<CircleToolState>,
    mut gizmos: Gizmos,
) {
    let Some((camera, camera_transform)) = camera_q.iter().next() else {
        return;
    };
    let Some(window) = windows.iter().next() else {
        return;
    };

    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(point) = camera.viewport_to_world_2d(camera_transform, cursor_pos) {
            if mouse.just_pressed(MouseButton::Left) {
                state.start_pos = Some(point);
                state.current_pos = Some(point);
            }

            if mouse.pressed(MouseButton::Left) {
                state.current_pos = Some(point);
            }

            if mouse.just_released(MouseButton::Left) {
                if let (Some(start), Some(end)) = (state.start_pos, state.current_pos) {
                    let radius = start.distance(end);

                    if radius > 5.0 {
                        let cmd = CreateCircleCommand {
                            position: start,
                            radius,
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

    if let (Some(start), Some(end)) = (state.start_pos, state.current_pos) {
        let radius = start.distance(end);
        gizmos.circle_2d(start, radius, Color::WHITE);
    }
}

struct CreateCircleCommand {
    position: Vec2,
    radius: f32,
    entity: Option<Entity>,
}

impl GameCommand for CreateCircleCommand {
    fn execute(&mut self, world: &mut World) {
        // Using Sprite as Mesh2d/MaterialMesh2dBundle usage is complex in this mixed version env.
        // It will look like a square, but physics will be circle.
        // Acceptable fallback given constraints.
        let id = world
            .spawn((
                RigidBody::Dynamic,
                Collider::circle(self.radius),
                Friction::default(),
                Restitution::new(0.5),
                Sprite {
                    color: Color::srgb(0.9, 0.4, 0.3),
                    custom_size: Some(Vec2::splat(self.radius * 2.0)),
                    ..default()
                },
                Transform::from_xyz(self.position.x, self.position.y, 0.0),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    #[fixture]
    fn world() -> World {
        World::new()
    }

    #[rstest]
    fn test_create_circle_command(mut world: World) {
        let mut cmd = CreateCircleCommand {
            position: Vec2::new(50.0, 60.0),
            radius: 15.0,
            entity: None,
        };

        cmd.execute(&mut world);

        let entity = cmd.entity.expect("Entity should be set");
        assert!(world.get_entity(entity).is_ok());

        let transform = world.get::<Transform>(entity).unwrap();
        assert_eq!(transform.translation, Vec3::new(50.0, 60.0, 0.0));

        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<Sprite>(entity).is_some());

        cmd.undo(&mut world);
        assert!(world.get_entity(entity).is_err());
    }
}
