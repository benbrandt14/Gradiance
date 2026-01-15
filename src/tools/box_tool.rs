use crate::commands::{GameCommand, SubmitGameCommand};
use crate::tools::ToolState;
use avian2d::prelude::*;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;

pub struct BoxToolPlugin;

impl Plugin for BoxToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoxToolState>();
        app.add_systems(Update, box_tool_logic.run_if(in_state(ToolState::Box)));
    }
}

#[derive(Resource, Default)]
struct BoxToolState {
    start_pos: Option<Vec2>,
    current_pos: Option<Vec2>,
}

fn box_tool_logic(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut state: ResMut<BoxToolState>,
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
                    let min = start.min(end);
                    let max = start.max(end);
                    let size = max - min;
                    let center = min + size / 2.0;

                    // Avoid tiny boxes
                    if size.x > 5.0 && size.y > 5.0 {
                        let cmd = CreateBoxCommand {
                            position: center,
                            size,
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
        let min = start.min(end);
        let max = start.max(end);
        let size = max - min;
        let center = min + size / 2.0;
        gizmos.rect_2d(center, size, Color::WHITE);
    }
}

// Command Implementation
struct CreateBoxCommand {
    position: Vec2,
    size: Vec2,
    entity: Option<Entity>,
}

impl GameCommand for CreateBoxCommand {
    fn execute(&mut self, world: &mut World) {
        let id = world
            .spawn((
                RigidBody::Dynamic,
                Collider::rectangle(self.size.x, self.size.y),
                Friction::default(),
                Restitution::new(0.5),
                Transform::from_xyz(self.position.x, self.position.y, 0.0),
                Sprite::from_color(Color::srgb(0.2, 0.7, 0.9), self.size),
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
    fn test_create_box_command(mut world: World) {
        let mut cmd = CreateBoxCommand {
            position: Vec2::new(10.0, 20.0),
            size: Vec2::new(30.0, 40.0),
            entity: None,
        };

        // Execute
        cmd.execute(&mut world);

        let entity = cmd.entity.expect("Entity should be set");
        assert!(world.get_entity(entity).is_ok());

        let transform = world.get::<Transform>(entity).unwrap();
        assert_eq!(transform.translation, Vec3::new(10.0, 20.0, 0.0));

        // Check for Collider presence (validating exact shape properties is harder without physics context)
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<RigidBody>(entity).is_some());

        // Undo
        cmd.undo(&mut world);
        assert!(world.get_entity(entity).is_err());
    }
}
