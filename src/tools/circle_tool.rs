use bevy::prelude::*;
use bevy::input::mouse::MouseButton;
use avian2d::prelude::*;
use crate::tools::ToolState;
use crate::commands::{GameCommand, SubmitGameCommand};

pub struct CircleToolPlugin;

impl Plugin for CircleToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CircleToolState>();
        app.add_systems(Update, circle_tool_logic.run_if(in_state(ToolState::Circle)));
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
    let Ok((camera, camera_transform)) = camera_q.get_single() else { return };
    let Ok(window) = windows.get_single() else { return };

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
        let mesh_handle = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            meshes.add(Circle::new(self.radius))
        };

        let mat_handle = {
            let mut materials = world.resource_mut::<Assets<ColorMaterial>>();
            materials.add(Color::srgb(0.9, 0.4, 0.3))
        };

        let id = world.spawn((
            RigidBody::Dynamic,
            Collider::circle(self.radius),
            Mesh2d(mesh_handle),
            MeshMaterial2d(mat_handle),
            Transform::from_xyz(self.position.x, self.position.y, 0.0),
        )).id();
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
        let mut w = World::new();
        w.init_resource::<Assets<Mesh>>();
        w.init_resource::<Assets<ColorMaterial>>();
        w
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
        assert!(world.get::<Mesh2d>(entity).is_some());

        cmd.undo(&mut world);
        assert!(world.get_entity(entity).is_err());
    }
}
