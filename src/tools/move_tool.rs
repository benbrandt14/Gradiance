use crate::commands::{BatchCommand, DeleteCommand, GameCommand, SubmitGameCommand};
use crate::tools::ToolState;
use avian2d::parry::shape::TypedShape;
use avian2d::prelude::*;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::utils::{HashMap, HashSet};
use bevy_egui::EguiContexts;

pub struct MoveToolPlugin;

impl Plugin for MoveToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoveToolState>();
        app.add_systems(Update, move_tool_logic.run_if(in_state(ToolState::Move)));
    }
}

#[derive(Resource, Default)]
pub struct MoveToolState {
    mode: MoveMode,
    pub selected_entities: HashSet<Entity>,
}

#[derive(Default)]
enum MoveMode {
    #[default]
    None,
    Drag(DragData),
    Rotate(RotateData),
    Select(SelectData),
}

impl MoveMode {
    fn is_none(&self) -> bool {
        matches!(self, MoveMode::None)
    }
}

struct DragData {
    // Entities being dragged
    entities: Vec<Entity>,
    // Initial transforms for undo
    initial_transforms: HashMap<Entity, Transform>,
    // Offset from cursor start
    start_point: Vec2,
    // Original rigid body types to restore
    original_body_types: HashMap<Entity, RigidBody>,
    // For throwing
    last_mouse_pos: Vec2,
    current_velocity: Vec2,
}

struct RotateData {
    entity: Entity,
    _start_mouse_pos: Vec2,
    initial_rotation: f32,
    initial_mouse_angle: f32,
    initial_transform: Transform,
}

struct SelectData {
    start_pos: Vec2,
    current_pos: Vec2,
}

#[allow(clippy::too_many_arguments)]
fn move_tool_logic(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    spatial_query: SpatialQuery,
    mut state: ResMut<MoveToolState>,
    mut queries: Query<(
        &mut Transform,
        Option<&mut RigidBody>,
        Option<&mut LinearVelocity>,
        Option<&Collider>,
        Option<&Sprite>,
    )>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_q.get_single() else {
        return;
    };
    let Ok(window) = windows.get_single() else {
        return;
    };

    // Block interaction if over UI
    if contexts.ctx_mut().is_pointer_over_area() {
        return;
    }

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok(point) = camera.viewport_to_world_2d(camera_transform, cursor_pos) else {
        return;
    };

    // Draw Selection Gizmos
    for &entity in state.selected_entities.iter() {
        if let Ok((transform, _, _, collider_opt, sprite_opt)) = queries.get(entity) {
            let pos = transform.translation.truncate();
            let color = Color::srgb(1.0, 1.0, 0.0);
            let mut drawn = false;

            if let Some(collider) = collider_opt {
                // Try to use Parry shape inspection
                match collider.shape().as_typed_shape() {
                    TypedShape::Cuboid(c) => {
                        let half_size = Vec2::new(c.half_extents.x, c.half_extents.y);
                        // Draw rotated box manually using lines
                        let corners = [
                            Vec2::new(-half_size.x, -half_size.y),
                            Vec2::new(half_size.x, -half_size.y),
                            Vec2::new(half_size.x, half_size.y),
                            Vec2::new(-half_size.x, half_size.y),
                        ];

                        let world_corners: Vec<Vec2> = corners
                            .iter()
                            .map(|&c| transform.transform_point(c.extend(0.0)).truncate())
                            .collect();

                        for i in 0..4 {
                            gizmos.line_2d(world_corners[i], world_corners[(i + 1) % 4], color);
                        }
                        drawn = true;
                    }
                    TypedShape::Ball(b) => {
                        gizmos.circle_2d(pos, b.radius, color);
                        drawn = true;
                    }
                    TypedShape::HalfSpace(_) => {
                         // For infinite plane, draw a very long line?
                         // Or just rely on the Sprite fallback (which we made huge)
                         // Actually, Sprite fallback works better for "huge rect".
                         // But if we want to outline the collider...
                         // Let's fallback to sprite for now.
                    }
                    _ => {}
                }
            }

            if !drawn {
                if let Some(sprite) = sprite_opt {
                    if let Some(size) = sprite.custom_size {
                         let half_size = size / 2.0;
                         let corners = [
                            Vec2::new(-half_size.x, -half_size.y),
                            Vec2::new(half_size.x, -half_size.y),
                            Vec2::new(half_size.x, half_size.y),
                            Vec2::new(-half_size.x, half_size.y),
                        ];

                        let world_corners: Vec<Vec2> = corners
                            .iter()
                            .map(|&c| transform.transform_point(c.extend(0.0)).truncate())
                            .collect();

                        for i in 0..4 {
                            gizmos.line_2d(world_corners[i], world_corners[(i + 1) % 4], color);
                        }
                        drawn = true;
                    }
                }
            }

            // Fallback default
            if !drawn {
                gizmos.circle_2d(pos, 40.0, color);
            }
        }
    }

    // Handle Delete
    if (keyboard.just_pressed(KeyCode::Delete) || keyboard.just_pressed(KeyCode::Backspace))
        && !state.selected_entities.is_empty()
    {
        let mut cmds: Vec<Box<dyn GameCommand>> = Vec::new();
        for &entity in state.selected_entities.iter() {
            cmds.push(Box::new(DeleteCommand::new(entity)));
        }
        if !cmds.is_empty() {
            commands.queue(SubmitGameCommand(Box::new(BatchCommand(cmds))));
            state.selected_entities.clear();
        }
    }

    // Start Interaction
    if mouse.just_pressed(MouseButton::Left) {
        let hits = spatial_query.point_intersections(point, &SpatialQueryFilter::default());

        // Find best hit (highest Z)
        let mut best_hit = None;
        let mut max_z = f32::MIN;

        for &hit in &hits {
             if let Ok((transform, _, _, _, _)) = queries.get(hit) {
                 if transform.translation.z > max_z {
                     max_z = transform.translation.z;
                     best_hit = Some(hit);
                 }
             }
        }

        if let Some(hit) = best_hit {
            // Hit an entity
            if keyboard.pressed(KeyCode::ShiftLeft) {
                // Toggle Selection
                if state.selected_entities.contains(&hit) {
                    state.selected_entities.remove(&hit);
                } else {
                    state.selected_entities.insert(hit);
                }
            } else {
                // Single Select (if not already selected)
                if !state.selected_entities.contains(&hit) {
                    state.selected_entities.clear();
                    state.selected_entities.insert(hit);
                }
            }

            if !state.selected_entities.is_empty() {
                 if keyboard.pressed(KeyCode::ShiftLeft) && state.mode.is_none() {
                     // Check if only one entity is selected for rotation
                     if state.selected_entities.len() == 1 {
                        let entity = *state.selected_entities.iter().next().unwrap();
                         if let Ok((transform, _, _, _, _)) = queries.get(entity) {
                             let diff = point - transform.translation.truncate();
                             let initial_mouse_angle = diff.y.atan2(diff.x);
                             let (_, _, rotation_z) = transform.rotation.to_euler(EulerRot::XYZ);

                             state.mode = MoveMode::Rotate(RotateData {
                                 entity,
                                 _start_mouse_pos: point,
                                 initial_rotation: rotation_z,
                                 initial_mouse_angle,
                                 initial_transform: *transform,
                             });
                         }
                     }
                 } else {
                    // Start Drag Mode (Kinematic)
                    let mut entities = Vec::new();
                    let mut initial_transforms = HashMap::new();
                    let mut original_body_types = HashMap::new();

                    for &e in state.selected_entities.iter() {
                        if let Ok((transform, rb_opt, _, _, _)) = queries.get_mut(e) {
                            entities.push(e);
                            initial_transforms.insert(e, *transform);

                            if let Some(mut rb) = rb_opt {
                                original_body_types.insert(e, *rb);
                                *rb = RigidBody::Kinematic;
                            }
                        }
                    }

                    state.mode = MoveMode::Drag(DragData {
                        entities,
                        initial_transforms,
                        start_point: point,
                        original_body_types,
                        last_mouse_pos: point,
                        current_velocity: Vec2::ZERO,
                    });
                 }
            }
        } else {
            // Clicked on nothing
            if !keyboard.pressed(KeyCode::ShiftLeft) {
                state.selected_entities.clear();
            }
            // Start Select Mode
            state.mode = MoveMode::Select(SelectData {
                start_pos: point,
                current_pos: point,
            });
        }
    }

    // Update Interaction
    if mouse.pressed(MouseButton::Left) {
        match &mut state.mode {
            MoveMode::Drag(data) => {
                let delta = point - data.start_point;

                // Calculate velocity
                let frame_delta = point - data.last_mouse_pos;
                let dt = time.delta_secs();
                if dt > 0.0001 {
                    data.current_velocity = frame_delta / dt;
                }
                data.last_mouse_pos = point;

                for &entity in &data.entities {
                    if let Some(initial) = data.initial_transforms.get(&entity) {
                        if let Ok((mut transform, _, _, _, _)) = queries.get_mut(entity) {
                            transform.translation = initial.translation + delta.extend(0.0);
                        }
                    }
                }
            }
            MoveMode::Rotate(data) => {
                if let Ok((mut transform, _, _, _, _)) = queries.get_mut(data.entity) {
                    let diff = point - transform.translation.truncate();
                    let current_mouse_angle = diff.y.atan2(diff.x);
                    let angle_delta = current_mouse_angle - data.initial_mouse_angle;

                    let new_rotation = data.initial_rotation + angle_delta;
                    transform.rotation = Quat::from_rotation_z(new_rotation);
                }
            }
            MoveMode::Select(data) => {
                data.current_pos = point;
                // Draw Gizmo
                let min = data.start_pos.min(data.current_pos);
                let max = data.start_pos.max(data.current_pos);
                let size = max - min;
                let center = min + size / 2.0;
                gizmos.rect_2d(center, size, Color::srgb(0.0, 1.0, 1.0)); // Cyan for selection
            }
            MoveMode::None => {}
        }
    }

    // End Interaction
    if mouse.just_released(MouseButton::Left) {
        match &mut state.mode {
            MoveMode::Drag(data) => {
                let mut cmd_list: Vec<Box<dyn GameCommand>> = Vec::new();

                for &entity in &data.entities {
                    if let Ok((transform, rb_opt, lin_vel_opt, _, _)) = queries.get_mut(entity) {
                        // Restore Body Type
                        if let Some(mut rb) = rb_opt {
                            if let Some(&original) = data.original_body_types.get(&entity) {
                                *rb = original;
                            }
                        }

                        // Apply Velocity
                        if let Some(mut lin_vel) = lin_vel_opt {
                            lin_vel.0 = data.current_velocity;
                        }

                        // Submit Command
                        if let Some(initial) = data.initial_transforms.get(&entity) {
                             if transform.translation != initial.translation || transform.rotation != initial.rotation {
                                 cmd_list.push(Box::new(TransformCommand {
                                     entity,
                                     old_transform: *initial,
                                     new_transform: *transform,
                                 }));
                             }
                        }
                    }
                }

                if !cmd_list.is_empty() {
                    commands.queue(SubmitGameCommand(Box::new(BatchCommand(cmd_list))));
                }
            }
            MoveMode::Rotate(data) => {
                if let Ok((transform, _, _, _, _)) = queries.get(data.entity) {
                    if transform.rotation != data.initial_transform.rotation {
                        let cmd = TransformCommand {
                            entity: data.entity,
                            old_transform: data.initial_transform,
                            new_transform: *transform,
                        };
                        commands.queue(SubmitGameCommand(Box::new(cmd)));
                    }
                }
            }
            MoveMode::Select(data) => {
                 let min = data.start_pos.min(data.current_pos);
                 let max = data.start_pos.max(data.current_pos);
                 let size = max - min;
                 let center = min + size / 2.0;

                 // Spatial Query for selection
                 let hits = spatial_query.shape_intersections(
                     &Collider::rectangle(size.x, size.y),
                     center,
                     0.0,
                     &SpatialQueryFilter::default()
                 );

                 for hit in hits {
                     state.selected_entities.insert(hit);
                 }
            }
            MoveMode::None => {}
        }
        state.mode = MoveMode::None;
    }
}

pub struct TransformCommand {
    pub entity: Entity,
    pub old_transform: Transform,
    pub new_transform: Transform,
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
