use crate::commands::{GameCommand, SubmitGameCommand};
use crate::tools::ToolState;
use avian2d::prelude::*;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;

pub struct MoveToolPlugin;

impl Plugin for MoveToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoveToolState>();
        app.add_systems(Update, move_tool_logic.run_if(in_state(ToolState::Move)));
    }
}

#[derive(Resource, Default)]
struct MoveToolState {
    mode: MoveMode,
}

#[derive(Default)]
enum MoveMode {
    #[default]
    None,
    Drag(DragData),
    Rotate(RotateData),
}

struct DragData {
    cursor_body: Entity,
    joint: Entity,
    target_body: Entity,
    initial_transform: Transform,
}

struct RotateData {
    entity: Entity,
    _start_mouse_pos: Vec2,
    initial_rotation: f32,
    initial_mouse_angle: f32,
    initial_transform: Transform,
}

#[allow(clippy::too_many_arguments)]
fn move_tool_logic(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    spatial_query: SpatialQuery,
    mut state: ResMut<MoveToolState>,
    mut transforms: Query<&mut Transform>,
) {
    let Ok((camera, camera_transform)) = camera_q.get_single() else {
        return;
    };
    let Ok(window) = windows.get_single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok(point) = camera.viewport_to_world_2d(camera_transform, cursor_pos) else {
        return;
    };

    // Start Interaction
    if mouse.just_pressed(MouseButton::Left) {
        let hits = spatial_query.point_intersections(point, &SpatialQueryFilter::default());
        if let Some(&hit) = hits.first() {
            if let Ok(transform) = transforms.get(hit) {
                let initial_transform = *transform;

                if keyboard.pressed(KeyCode::ShiftLeft) {
                    // Rotate Mode
                    let diff = point - transform.translation.truncate();
                    let initial_mouse_angle = diff.y.atan2(diff.x);
                    let (_, _, rotation_z) = transform.rotation.to_euler(EulerRot::XYZ);

                    state.mode = MoveMode::Rotate(RotateData {
                        entity: hit,
                        _start_mouse_pos: point,
                        initial_rotation: rotation_z,
                        initial_mouse_angle,
                        initial_transform,
                    });
                } else {
                    // Drag Mode
                    let cursor_body = commands
                        .spawn((
                            RigidBody::Kinematic,
                            Transform::from_translation(point.extend(0.0)),
                        ))
                        .id();

                    let local_anchor = transform.rotation.inverse()
                        * (point - transform.translation.truncate()).extend(0.0);
                    let local_anchor_2d = local_anchor.truncate();

                    let joint = commands
                        .spawn(
                            DistanceJoint::new(cursor_body, hit)
                                .with_local_anchor_1(Vec2::ZERO)
                                .with_local_anchor_2(local_anchor_2d)
                                .with_rest_length(0.0)
                                .with_compliance(0.00001),
                        )
                        .id();

                    state.mode = MoveMode::Drag(DragData {
                        cursor_body,
                        joint,
                        target_body: hit,
                        initial_transform,
                    });
                }
            }
        }
    }

    // Update Interaction
    if mouse.pressed(MouseButton::Left) {
        match &mut state.mode {
            MoveMode::Drag(data) => {
                if let Ok(mut t) = transforms.get_mut(data.cursor_body) {
                    t.translation = point.extend(0.0);
                }
            }
            MoveMode::Rotate(data) => {
                if let Ok(mut t) = transforms.get_mut(data.entity) {
                    let diff = point - t.translation.truncate();
                    let current_mouse_angle = diff.y.atan2(diff.x);
                    let angle_delta = current_mouse_angle - data.initial_mouse_angle;

                    let new_rotation = data.initial_rotation + angle_delta;
                    t.rotation = Quat::from_rotation_z(new_rotation);
                }
            }
            MoveMode::None => {}
        }
    }

    // End Interaction
    if mouse.just_released(MouseButton::Left) {
        match &state.mode {
            MoveMode::Drag(data) => {
                commands.entity(data.cursor_body).despawn();
                commands.entity(data.joint).despawn();

                // Submit Command
                if let Ok(new_transform) = transforms.get(data.target_body) {
                    if new_transform.translation != data.initial_transform.translation
                        || new_transform.rotation != data.initial_transform.rotation
                    {
                        let cmd = TransformCommand {
                            entity: data.target_body,
                            old_transform: data.initial_transform,
                            new_transform: *new_transform,
                        };
                        commands.queue(SubmitGameCommand(Box::new(cmd)));
                    }
                }
            }
            MoveMode::Rotate(data) => {
                // Submit Command
                if let Ok(new_transform) = transforms.get(data.entity) {
                    if new_transform.rotation != data.initial_transform.rotation {
                        let cmd = TransformCommand {
                            entity: data.entity,
                            old_transform: data.initial_transform,
                            new_transform: *new_transform,
                        };
                        commands.queue(SubmitGameCommand(Box::new(cmd)));
                    }
                }
            }
            MoveMode::None => {}
        }
        state.mode = MoveMode::None;
    }
}

struct TransformCommand {
    entity: Entity,
    old_transform: Transform,
    new_transform: Transform,
}

impl GameCommand for TransformCommand {
    fn execute(&mut self, world: &mut World) {
        if let Some(mut t) = world.get_mut::<Transform>(self.entity) {
            *t = self.new_transform;
        }
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(mut t) = world.get_mut::<Transform>(self.entity) {
            *t = self.old_transform;
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
    fn test_transform_command(mut world: World) {
        let entity = world.spawn(Transform::from_xyz(0.0, 0.0, 0.0)).id();
        let old_transform = Transform::from_xyz(0.0, 0.0, 0.0);
        let new_transform = Transform::from_xyz(10.0, 10.0, 0.0);

        let mut cmd = TransformCommand {
            entity,
            old_transform,
            new_transform,
        };

        // Execute
        cmd.execute(&mut world);
        let t = world.get::<Transform>(entity).unwrap();
        assert_eq!(t.translation, Vec3::new(10.0, 10.0, 0.0));

        // Undo
        cmd.undo(&mut world);
        let t = world.get::<Transform>(entity).unwrap();
        assert_eq!(t.translation, Vec3::new(0.0, 0.0, 0.0));
    }
}
