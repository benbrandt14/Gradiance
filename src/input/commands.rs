//! Command pattern implementation for Undo/Redo.
//!
//! Defines the `GameCommand` trait and the `CommandStack` resource.

use crate::input::ZIndex;
use crate::input::editable::{EditableBox, EditableCircle};
use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::rapier::geometry::SharedShape;
use nalgebra::Point2;

/// A trait for game commands that support Undo/Redo.
pub trait GameCommand: Send + Sync {
    /// Apply the command to the world.
    fn apply(&mut self, world: &mut World) -> Result<(), String>;

    /// Revert the command's effects.
    fn undo(&mut self, world: &mut World);

    /// Returns the name of the command.
    fn name(&self) -> String;
}

/// Resource handling the stack of executed commands.
#[derive(Resource, Default)]
pub struct CommandStack {
    /// The stack of commands.
    history: Vec<Box<dyn GameCommand>>,
    /// The current position in the stack (points to the next slot to write).
    /// If index < history.len(), we are in a "Redo" state.
    index: usize,
}

impl CommandStack {
    /// Pushes a new command and executes it.
    /// Clears any redo history.
    pub fn push(&mut self, mut command: Box<dyn GameCommand>, world: &mut World) {
        // If we are in the middle of the stack (undo performed), clear the future.
        if self.index < self.history.len() {
            self.history.truncate(self.index);
        }

        match command.apply(world) {
            Ok(_) => {
                info!("Command Applied: {}", command.name());
                self.history.push(command);
                self.index += 1;
            }
            Err(e) => {
                warn!("Command Failed: {}: {}", command.name(), e);
            }
        }
    }

    /// Undoes the last command.
    pub fn undo(&mut self, world: &mut World) {
        if self.index > 0 {
            self.index -= 1;
            if let Some(command) = self.history.get_mut(self.index) {
                info!("Undo: {}", command.name());
                command.undo(world);
            }
        }
    }

    /// Redoes the previously undone command.
    pub fn redo(&mut self, world: &mut World) {
        if self.index < self.history.len() {
            if let Some(command) = self.history.get_mut(self.index) {
                if let Err(e) = command.apply(world) {
                    warn!("Redo Failed: {}: {}", command.name(), e);
                } else {
                    info!("Redo: {}", command.name());
                }
            }
            self.index += 1;
        }
    }
}

/// Command to spawn a box.
pub struct SpawnBoxCommand {
    /// Position of the box.
    pub position: Vec2,
    /// Width of the box.
    pub width: f32,
    /// Height of the box.
    pub height: f32,
    /// The spawned entity ID (if active).
    pub entity: Option<Entity>,
}

impl SpawnBoxCommand {
    /// Create a new SpawnBoxCommand.
    pub fn new(position: Vec2, width: f32, height: f32) -> Self {
        Self {
            position,
            width,
            height,
            entity: None,
        }
    }
}

impl GameCommand for SpawnBoxCommand {
    fn name(&self) -> String {
        "Spawn Box".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let z = world.resource_mut::<ZIndex>().next();

        let entity = world
            .spawn((
                Transform::from_xyz(self.position.x, self.position.y, z),
                GlobalTransform::default(),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
                RigidBody::Dynamic,
                // Rapier uses half-extents
                Collider::cuboid(self.width / 2.0, self.height / 2.0),
                EditableBox {
                    width: self.width as f64,
                    height: self.height as f64,
                },
            ))
            .id();

        self.entity = Some(entity);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(entity) = self.entity {
            if let Ok(entity_ref) = world.get_entity_mut(entity) {
                entity_ref.despawn();
            }
            self.entity = None;
        }
    }
}

/// Command to spawn a circle.
pub struct SpawnCircleCommand {
    /// The center position.
    pub position: Vec2,
    /// The radius.
    pub radius: f32,
    /// The spawned entity ID.
    pub entity: Option<Entity>,
}

impl GameCommand for SpawnCircleCommand {
    fn name(&self) -> String {
        "Spawn Circle".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let z = world.resource_mut::<ZIndex>().next();

        let id = world
            .spawn((
                Transform::from_xyz(self.position.x, self.position.y, z),
                GlobalTransform::default(),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
                RigidBody::Dynamic,
                Collider::ball(self.radius),
                EditableCircle {
                    radius: self.radius as f64,
                },
            ))
            .id();

        self.entity = Some(id);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(e) = self.entity {
            if let Ok(e_ref) = world.get_entity_mut(e) {
                e_ref.despawn();
            }
            self.entity = None;
        }
    }
}

/// Command to spawn a polygon.
pub struct SpawnPolygonCommand {
    /// The center position.
    pub position: Vec2,
    /// The vertices relative to the center.
    pub vertices: Vec<Vec2>,
    /// The spawned entity ID.
    pub entity: Option<Entity>,
}

impl GameCommand for SpawnPolygonCommand {
    fn name(&self) -> String {
        "Spawn Polygon".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        if self.vertices.len() < 3 {
            return Err("Polygon must have at least 3 vertices".to_string());
        }

        let z = world.resource_mut::<ZIndex>().next();

        let vertices: Vec<Point2<f32>> = self.vertices
            .iter()
            .map(|v| Point2::new(v.x, v.y))
            .collect();
        let indices: Vec<[u32; 2]> = (0..vertices.len())
            .map(|i| [i as u32, ((i + 1) % vertices.len()) as u32])
            .collect();

        let rapier_shape = SharedShape::convex_decomposition(&vertices, &indices);
        let collider = Collider::from(rapier_shape);

        let id = world
            .spawn((
                Transform::from_xyz(self.position.x, self.position.y, z),
                GlobalTransform::default(),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
                RigidBody::Dynamic,
                collider,
            ))
            .id();

        self.entity = Some(id);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(e) = self.entity {
            if let Ok(e_ref) = world.get_entity_mut(e) {
                e_ref.despawn();
            }
            self.entity = None;
        }
    }
}

/// Command to spawn a Revolute Joint (Hinge).
pub struct SpawnJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional, None means World/Pin).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// Joint compliance (ignored in rigid Rapier joints usually, or mapped to stiffness).
    pub compliance: f32,
    /// The pin entity ID (if pinning to world).
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnJointCommand {
    fn name(&self) -> String {
        "Spawn Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        // Physics Joint
        let target_entity;
        let joint_data;

        if let Some(e_b) = self.entity_b {
            target_entity = e_b;
            joint_data = RevoluteJointBuilder::new()
                .local_anchor1(self.anchor_a)
                .local_anchor2(self.anchor_b);
        } else {
            // Pin logic
            let t_a = world
                .get::<GlobalTransform>(self.entity_a)
                .map(|t| t.compute_transform())
                .unwrap_or_default();
            let world_pos = t_a.transform_point(Vec3::new(self.anchor_a.x, self.anchor_a.y, 0.0));

            let pin_id = world
                .spawn((RigidBody::Fixed, Transform::from_translation(world_pos)))
                .id();

            self.pin_entity = Some(pin_id);
            target_entity = pin_id;

            joint_data = RevoluteJointBuilder::new()
                .local_anchor1(self.anchor_a)
                .local_anchor2(Vec2::ZERO);
        }

        // Attach ImpulseJoint to entity_a
        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        // Remove Joint from entity_a
        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<ImpulseJoint>();
        }

        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p) {
                e.despawn();
            }
            self.pin_entity = None;
        }
    }
}

/// Command to spawn a Fixed Joint (Weld).
pub struct SpawnFixedJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// Joint compliance.
    pub compliance: f32,
    /// The pin entity ID.
    pub pin_entity: Option<Entity>,
    /// Rotation of body A (radians).
    pub rot_a: f32,
    /// Rotation of body B (radians).
    pub rot_b: f32,
}

impl GameCommand for SpawnFixedJointCommand {
    fn name(&self) -> String {
        "Spawn Fixed Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let target_entity;
        let joint_data;

        if let Some(e_b) = self.entity_b {
            target_entity = e_b;
            joint_data = FixedJointBuilder::new()
                .local_anchor1(self.anchor_a)
                .local_anchor2(self.anchor_b);
        } else {
            let t_a = world
                .get::<GlobalTransform>(self.entity_a)
                .map(|t| t.compute_transform())
                .unwrap_or_default();
            let world_pos = t_a.transform_point(Vec3::new(self.anchor_a.x, self.anchor_a.y, 0.0));

            let pin_id = world
                .spawn((RigidBody::Fixed, Transform::from_translation(world_pos)))
                .id();

            self.pin_entity = Some(pin_id);
            target_entity = pin_id;

            joint_data = FixedJointBuilder::new()
                .local_anchor1(self.anchor_a)
                .local_anchor2(Vec2::ZERO);
        }

        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<ImpulseJoint>();
        }

        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p) {
                e.despawn();
            }
            self.pin_entity = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use bevy_rapier2d::prelude::*;
    use rstest::{fixture, rstest};
    use crate::input::ZIndex as GameZIndex;

    #[fixture]
    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<GameZIndex>();
        world
    }

    #[rstest]
    fn test_spawn_polygon_command_failure(mut world: World) {
        let vertices = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
        ]; // Only 2 vertices
        let mut cmd = SpawnPolygonCommand {
            position: Vec2::new(0.0, 0.0),
            vertices: vertices.clone(),
            entity: None,
        };

        // Apply should fail
        let result = cmd.apply(&mut world);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Polygon must have at least 3 vertices");
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_box_command(mut world: World) {
        let mut cmd = SpawnBoxCommand::new(Vec2::new(10.0, 20.0), 5.0, 5.0);

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        let transform = world.get::<Transform>(entity);
        assert!(transform.is_some());
        assert_eq!(transform.unwrap().translation.truncate(), Vec2::new(10.0, 20.0));

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<EditableBox>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        // In Bevy 0.15+, get_entity returns a Result. If despawned, it should be Err.
        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_circle_command(mut world: World) {
        let mut cmd = SpawnCircleCommand {
            position: Vec2::new(-5.0, 5.0),
            radius: 3.0,
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        let transform = world.get::<Transform>(entity);
        assert!(transform.is_some());
        assert_eq!(transform.unwrap().translation.truncate(), Vec2::new(-5.0, 5.0));

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<EditableCircle>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_joint_command(mut world: World) {
        // Setup entity_a
        let entity_a = world.spawn(Transform::default()).id();

        let mut cmd = SpawnJointCommand {
            entity_a,
            entity_b: None, // Pin to world
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            compliance: 0.0,
            pin_entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        // Check ImpulseJoint on entity_a
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        // Check visual entity NOT spawned. We can't easily check for "no children" because we don't know the world state fully,
        // but we can check there are no children if we assume a fresh entity_a.
        if let Some(children) = world.get::<Children>(entity_a) {
             assert!(children.is_empty(), "Joint should not have visual children");
        }

        // Check pin entity
        assert!(cmd.pin_entity.is_some());
        let pin_id = cmd.pin_entity.unwrap();
        assert!(world.get::<RigidBody>(pin_id).is_some());

        // Undo
        cmd.undo(&mut world);

        // Check ImpulseJoint removed
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());

        // Check pin entity despawned
        assert!(world.get_entity(pin_id).is_err());
    }

    #[rstest]
    fn test_spawn_polygon_command(mut world: World) {
        let vertices = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 10.0),
        ];
        let mut cmd = SpawnPolygonCommand {
            position: Vec2::new(0.0, 0.0),
            vertices: vertices.clone(),
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        // Verify transform exists
        assert!(world.get::<Transform>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_command_stack(mut world: World) {
        let mut stack = CommandStack::default();

        // 1. Push Box
        let box_cmd = Box::new(SpawnBoxCommand::new(Vec2::ZERO, 1.0, 1.0));
        stack.push(box_cmd, &mut world);

        assert_eq!(stack.index, 1);
        assert_eq!(stack.history.len(), 1);
        assert_eq!(world.entities().len(), 1);

        // 2. Undo
        stack.undo(&mut world);
        assert_eq!(stack.index, 0);
        assert_eq!(stack.history.len(), 1);
        assert_eq!(world.entities().len(), 0);

        // 3. Redo
        stack.redo(&mut world);
        assert_eq!(stack.index, 1);
        assert_eq!(world.entities().len(), 1);

        // 4. Undo again
        stack.undo(&mut world);
        assert_eq!(stack.index, 0);
        assert_eq!(world.entities().len(), 0);

        // 5. Push new command (Circle), should truncate history
        let circle_cmd = Box::new(SpawnCircleCommand {
            position: Vec2::new(10.0, 0.0),
            radius: 1.0,
            entity: None,
        });
        stack.push(circle_cmd, &mut world);

        assert_eq!(stack.index, 1);
        assert_eq!(stack.history.len(), 1); // Previous box command should be removed
        assert_eq!(world.entities().len(), 1);

        // Verify it is indeed the circle (by checking component)
        let entity = world.iter_entities().next().unwrap().id();
        assert!(world.get::<EditableCircle>(entity).is_some());
    }
}
