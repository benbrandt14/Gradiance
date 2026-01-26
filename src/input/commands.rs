//! Command pattern implementation for Undo/Redo.
//!
//! Defines the [`GameCommand`] trait and the [`CommandStack`] resource.

use crate::geometry::mesh_generator::generate_3d_mesh;
use crate::input::ZIndex;
use crate::input::editable_shape::{EditableShape, ShapeType, generate_shape_components};
use crate::input::tools::connector::Connector;
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use anyhow::{Result, bail};
// use bevy_prototype_lyon::prelude::*; // Unused
use std::fmt::Debug;

const DEFAULT_BOX_COLOR: Color = Color::srgb(0.5, 0.5, 1.0);
const DEFAULT_CIRCLE_COLOR: Color = Color::srgb(1.0, 0.5, 0.5);
const DEFAULT_POLYGON_COLOR: Color = Color::srgb(0.5, 1.0, 0.5);
const DEFAULT_GROUND_COLOR: Color = Color::srgb(0.2, 0.2, 0.2);
const GROUND_WIDTH: f32 = 100_000.0;
const GROUND_DEPTH: f32 = 1000.0;
const CONNECTOR_COLLIDER_RADIUS: f32 = 0.5;
const VISUAL_CIRCLE_OUTER_RADIUS: f32 = 0.5;
const VISUAL_CIRCLE_INNER_RADIUS: f32 = 0.2;
const VISUAL_LINE_OFFSET: f32 = 3.0;

/// A trait for game commands that support Undo/Redo.
pub trait GameCommand: Send + Sync + Debug {
    /// Apply the command to the world.
    ///
    /// # Errors
    /// Returns an error if the command cannot be applied (e.g., invalid geometry).
    fn apply(&mut self, world: &mut World) -> Result<()>;

    /// Revert the command's effects.
    fn undo(&mut self, world: &mut World);

    /// Returns the name of the command.
    fn name(&self) -> String;
}

/// Resource handling the stack of executed commands.
#[derive(Resource, Default, Debug)]
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
            Ok(()) => {
                info!(name = %command.name(), "Command Applied");
                self.history.push(command);
                self.index += 1;
            }
            Err(e) => {
                warn!(name = %command.name(), error = %e, "Command Failed");
            }
        }
    }

    /// Undoes the last command.
    pub fn undo(&mut self, world: &mut World) {
        if self.index > 0 {
            self.index -= 1;
            if let Some(command) = self.history.get_mut(self.index) {
                info!(name = %command.name(), "Undo");
                command.undo(world);
            }
        }
    }

    /// Redoes the previously undone command.
    pub fn redo(&mut self, world: &mut World) {
        if self.index < self.history.len() {
            if let Some(command) = self.history.get_mut(self.index) {
                if let Err(e) = command.apply(world) {
                    warn!(name = %command.name(), error = %e, "Redo Failed");
                } else {
                    info!(name = %command.name(), "Redo");
                }
            }
            self.index += 1;
        }
    }
}

/// Helper to resolve joint targets and handle pinning.
fn resolve_joint_targets(
    world: &mut World,
    entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    visual_entity: Option<Entity>,
) -> (Entity, Option<Entity>, Vec2, Vec2) {
    let mut pin_entity = None;
    let target_entity;
    let local_anchor_1 = anchor_a;
    let local_anchor_2;

    if let Some(e_b) = entity_b {
        target_entity = e_b;
        local_anchor_2 = anchor_b;
    } else {
        // Pin logic
        let t_a = world
            .get::<GlobalTransform>(entity_a)
            .map(|t| t.compute_transform())
            .unwrap_or_default();
        let world_pos = t_a.transform_point(Vec3::new(anchor_a.x, anchor_a.y, 0.0));

        let pin_id = world
            .spawn((
                RigidBody::Fixed,
                Transform::from_translation(world_pos),
                CollisionGroups::new(Group::NONE, Group::NONE),
            ))
            .id();

        pin_entity = Some(pin_id);
        target_entity = pin_id;
        local_anchor_2 = Vec2::ZERO;

        if let Some(v_id) = visual_entity
            && let Some(mut connector) = world.get_mut::<Connector>(v_id)
        {
            connector.entity_b = Some(pin_id);
        }
    }

    (target_entity, pin_entity, local_anchor_1, local_anchor_2)
}

/// Helper to spawn connector visual.
fn spawn_connector_visual(
    world: &mut World,
    entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    children_builder: impl FnOnce(&mut World, Entity),
) -> Entity {
    let visual_id = world
        .spawn((
            Transform::from_xyz(anchor_a.x, anchor_a.y, 0.1),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Collider::ball(CONNECTOR_COLLIDER_RADIUS),
            Sensor,
            Connector {
                entity_a,
                entity_b,
                local_anchor_a: anchor_a,
                local_anchor_b: anchor_b,
            },
        ))
        .set_parent_in_place(entity_a)
        .id();

    children_builder(world, visual_id);

    visual_id
}

/// Command to spawn a generic shape.
#[derive(Debug)]
pub struct SpawnShapeCommand {
    /// Position of the shape.
    pub position: Vec2,
    /// The shape definition.
    pub shape: ShapeType,
    /// The spawned entity ID.
    pub entity: Option<Entity>,
}

impl GameCommand for SpawnShapeCommand {
    fn name(&self) -> String {
        match &self.shape {
            ShapeType::Box { .. } => "Spawn Box".to_string(),
            ShapeType::Circle { .. } => "Spawn Circle".to_string(),
            ShapeType::Polygon { .. } => "Spawn Polygon".to_string(),
        }
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
        let Some((path, collider)) = generate_shape_components(&self.shape) else {
            bail!("Invalid shape parameters");
        };

        let z = world.resource_mut::<ZIndex>().next_z();
        let fill_color = match self.shape {
            ShapeType::Box { .. } => DEFAULT_BOX_COLOR,
            ShapeType::Circle { .. } => DEFAULT_CIRCLE_COLOR,
            ShapeType::Polygon { .. } => DEFAULT_POLYGON_COLOR,
        };

        let mesh = generate_3d_mesh(&self.shape);
        let mesh_handle = world.resource_mut::<Assets<Mesh>>().add(mesh);

        let material = StandardMaterial {
            base_color: fill_color,
            unlit: false,
            ..default()
        };
        let material_handle = world.resource_mut::<Assets<StandardMaterial>>().add(material);

        let entity = world
            .spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::from_xyz(self.position.x, self.position.y, z),
                RigidBody::Dynamic,
                collider,
                EditableShape {
                    shape: self.shape.clone(),
                },
                path,
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

/// Command to spawn a Revolute Joint (Hinge).
#[derive(Debug)]
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
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID (if pinning to world).
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnJointCommand {
    fn name(&self) -> String {
        "Spawn Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
        // Visual
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                // Outer Sphere
                let outer_mesh = Mesh::from(Sphere::new(VISUAL_CIRCLE_OUTER_RADIUS));
                let outer_mesh_h = world.resource_mut::<Assets<Mesh>>().add(outer_mesh);
                let outer_mat = StandardMaterial {
                    base_color: Color::BLACK,
                    ..default()
                };
                let outer_mat_h = world.resource_mut::<Assets<StandardMaterial>>().add(outer_mat);

                world.entity_mut(visual_id).insert((
                    Mesh3d(outer_mesh_h),
                    MeshMaterial3d(outer_mat_h),
                ));

                // Inner Sphere
                let inner_mesh = Mesh::from(Sphere::new(VISUAL_CIRCLE_INNER_RADIUS));
                let inner_mesh_h = world.resource_mut::<Assets<Mesh>>().add(inner_mesh);
                let inner_mat = StandardMaterial {
                    base_color: Color::WHITE,
                    ..default()
                };
                let inner_mat_h = world.resource_mut::<Assets<StandardMaterial>>().add(inner_mat);

                let inner = world
                    .spawn((
                        Mesh3d(inner_mesh_h),
                        MeshMaterial3d(inner_mat_h),
                        Transform::from_translation(Vec3::Z * 0.2), // Pop out slightly
                    ))
                    .id();
                world.entity_mut(visual_id).add_child(inner);
            },
        );
        self.visual_entity = Some(visual_id);

        // Physics Joint
        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
        );
        self.pin_entity = pin_entity;

        let joint_data = RevoluteJointBuilder::new()
            .local_anchor1(local_anchor_1)
            .local_anchor2(local_anchor_2);

        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }

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
#[derive(Debug)]
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
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
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

    fn apply(&mut self, world: &mut World) -> Result<()> {
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                // Cross visuals: Two cuboids
                // Line 1
                let line_len = VISUAL_LINE_OFFSET * 2.0; // Approx
                let thickness = 0.4;
                // We use Cuboid.
                // We need to rotate them to form an X.
                // 45 degrees
                let mesh = Mesh::from(Cuboid::new(line_len * 1.414, thickness, 0.4)); // length sqrt(2)*offset?
                // Logic in old code was Line from (-O,-O) to (O,O). Length is sqrt(8)*O.

                let mesh_h = world.resource_mut::<Assets<Mesh>>().add(mesh);
                let mat = StandardMaterial {
                    base_color: Color::srgb(1.0, 0.0, 0.0),
                    ..default()
                };
                let mat_h = world.resource_mut::<Assets<StandardMaterial>>().add(mat);

                let v1 = world
                    .spawn((
                        Mesh3d(mesh_h.clone()),
                        MeshMaterial3d(mat_h.clone()),
                        Transform::from_translation(Vec3::Z * 0.1)
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
                    ))
                    .id();

                let v2 = world
                    .spawn((
                        Mesh3d(mesh_h),
                        MeshMaterial3d(mat_h),
                        Transform::from_translation(Vec3::Z * 0.1)
                            .with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_4)),
                    ))
                    .id();
                world.entity_mut(visual_id).add_children(&[v1, v2]);
            },
        );
        self.visual_entity = Some(visual_id);

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
        );
        self.pin_entity = pin_entity;

        let joint_data = FixedJointBuilder::new()
            .local_anchor1(local_anchor_1)
            .local_anchor2(local_anchor_2)
            .local_basis1(-self.rot_a)
            .local_basis2(-self.rot_b);

        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }

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

/// Command to spawn an infinite ground plane.
#[derive(Debug)]
pub struct SpawnGroundCommand {
    /// Position of the ground (center of surface).
    pub position: Vec2,
    /// Rotation angle (radians).
    pub rotation: f32,
    /// The spawned entity ID.
    pub entity: Option<Entity>,
}

impl GameCommand for SpawnGroundCommand {
    fn name(&self) -> String {
        "Spawn Ground".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
        // Offset: the visual rectangle is centered at (0,0).
        // The collider is centered at (0,0).
        // But we want the "surface" (top edge) to be at `position` aligned with `rotation`.
        // The box center is at (0, -depth/2) relative to the surface.
        // So we rotate (0, -depth/2) by `rotation` and add to `position`.

        let rot = Quat::from_rotation_z(self.rotation);
        let offset = rot * Vec3::new(0.0, -GROUND_DEPTH / 2.0, 0.0);
        let center = Vec3::new(self.position.x, self.position.y, 0.0) + offset;

        let z = -1.0; // Force ground to be behind

        let mesh = Mesh::from(Cuboid::new(GROUND_WIDTH, GROUND_DEPTH, 1.0));
        let mesh_handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = StandardMaterial {
            base_color: DEFAULT_GROUND_COLOR,
            unlit: false,
            ..default()
        };
        let material_handle = world.resource_mut::<Assets<StandardMaterial>>().add(material);

        let entity = world
            .spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::from_translation(center + Vec3::new(0.0, 0.0, z))
                    .with_rotation(rot),
                // Physics
                RigidBody::Fixed,
                Collider::cuboid(GROUND_WIDTH / 2.0, GROUND_DEPTH / 2.0),
                Friction::coefficient(0.5),
                Restitution::coefficient(0.0),
                GroundPlane,
                Name::new("Ground"),
            ))
            .id();

        self.entity = Some(entity);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ZIndex as GameZIndex;
    use crate::input::tools::connector::Connector;
    use bevy::prelude::*;
    use bevy_rapier2d::prelude::*;
    use rstest::{fixture, rstest};

    #[fixture]
    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<GameZIndex>();
        // Add assets for mesh generation
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<Assets<Image>>(); // Needed? Maybe for StandardMaterial default
        world
    }

    #[rstest]
    fn test_spawn_polygon_command_failure(mut world: World) {
        let vertices = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]; // Only 2 vertices
        let mut cmd = SpawnShapeCommand {
            position: Vec2::new(0.0, 0.0),
            shape: ShapeType::Polygon { points: vertices },
            entity: None,
        };

        // Apply should fail (generate_shape_components returns None)
        let result = cmd.apply(&mut world);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid shape parameters");
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_box_command(mut world: World) {
        let mut cmd = SpawnShapeCommand {
            position: Vec2::new(10.0, 20.0),
            shape: ShapeType::Box {
                width: 5.0,
                height: 5.0,
            },
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        let transform = world.get::<Transform>(entity);
        assert!(transform.is_some());
        assert_eq!(
            transform.unwrap().translation.truncate(),
            Vec2::new(10.0, 20.0)
        );

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<EditableShape>(entity).is_some());
        assert!(world.get::<Mesh3d>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        // In Bevy 0.15+, get_entity returns a Result. If despawned, it should be Err.
        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_circle_command(mut world: World) {
        let mut cmd = SpawnShapeCommand {
            position: Vec2::new(-5.0, 5.0),
            shape: ShapeType::Circle { radius: 3.0 },
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        let transform = world.get::<Transform>(entity);
        assert!(transform.is_some());
        assert_eq!(
            transform.unwrap().translation.truncate(),
            Vec2::new(-5.0, 5.0)
        );

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<EditableShape>(entity).is_some());
        assert!(world.get::<Mesh3d>(entity).is_some());

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
            visual_entity: None,
            pin_entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        // Check ImpulseJoint on entity_a
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        // Check visual entity spawned (child of entity_a)
        let children = world.get::<Children>(entity_a);
        assert!(children.is_some());
        // Since we don't know if there are other children, we look for one with Connector
        let visual_id = children
            .unwrap()
            .iter()
            .find(|&&child| world.get::<Connector>(child).is_some());
        assert!(visual_id.is_some());
        let visual_id = *visual_id.unwrap();

        // Check Mesh3d (Fixed joint has visuals as children)
        let v_children = world.get::<Children>(visual_id);
        assert!(v_children.is_some());
        assert!(v_children.unwrap().iter().any(|&c| world.get::<Mesh3d>(c).is_some()));

        // Check pin entity
        assert!(cmd.pin_entity.is_some());
        let pin_id = cmd.pin_entity.unwrap();
        assert!(world.get::<RigidBody>(pin_id).is_some());

        // Undo
        cmd.undo(&mut world);

        // Check ImpulseJoint removed
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());

        // Check visual entity despawned
        assert!(world.get_entity(visual_id).is_err());

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
        let mut cmd = SpawnShapeCommand {
            position: Vec2::new(0.0, 0.0),
            shape: ShapeType::Polygon { points: vertices },
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        // Verify Mesh3d
        assert!(world.get::<Mesh3d>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_command_stack(mut world: World) {
        let mut stack = CommandStack::default();

        // 1. Push Box
        let box_cmd = Box::new(SpawnShapeCommand {
            position: Vec2::ZERO,
            shape: ShapeType::Box {
                width: 1.0,
                height: 1.0,
            },
            entity: None,        });
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
        let circle_cmd = Box::new(SpawnShapeCommand {
            position: Vec2::new(10.0, 0.0),
            shape: ShapeType::Circle { radius: 1.0 },
            entity: None,
        });
        stack.push(circle_cmd, &mut world);

        assert_eq!(stack.index, 1);
        assert_eq!(stack.history.len(), 1); // Previous box command should be removed
        assert_eq!(world.entities().len(), 1);

        // Verify it is indeed the circle (by checking component)
        let entity = world.iter_entities().next().unwrap().id();
        assert!(world.get::<EditableShape>(entity).is_some());
    }

    #[rstest]
    fn test_spawn_fixed_joint_command(mut world: World) {
        let entity_a = world.spawn(Transform::default()).id();

        let mut cmd = SpawnFixedJointCommand {
            entity_a,
            entity_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            compliance: 0.0,
            visual_entity: None,
            pin_entity: None,
            rot_a: 0.0,
            rot_b: 0.0,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        // Check ImpulseJoint
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        // Check visual entity
        let children = world.get::<Children>(entity_a);
        assert!(children.is_some());
        let visual_id = *children
            .unwrap()
            .iter()
            .find(|&&child| world.get::<Connector>(child).is_some())
            .unwrap();

        // Check Mesh3d (Fixed joint has visuals as children)
        let v_children = world.get::<Children>(visual_id);
        assert!(v_children.is_some());
        assert!(v_children.unwrap().iter().any(|&c| world.get::<Mesh3d>(c).is_some()));

        // Check pin entity
        assert!(cmd.pin_entity.is_some());
        let pin_id = cmd.pin_entity.unwrap();
        assert!(world.get::<RigidBody>(pin_id).is_some());

        // Undo
        cmd.undo(&mut world);

        // Check ImpulseJoint removed
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());

        // Check entities despawned
        assert!(world.get_entity(pin_id).is_err());
        assert!(world.get_entity(visual_id).is_err());
    }

    #[rstest]
    fn test_spawn_joint_command_two_bodies(mut world: World) {
        let entity_a = world.spawn(Transform::default()).id();
        let entity_b = world.spawn(Transform::default()).id();

        let mut cmd = SpawnJointCommand {
            entity_a,
            entity_b: Some(entity_b),
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            compliance: 0.0,
            visual_entity: None,
            pin_entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        // Check ImpulseJoint on entity_a
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        // Verify joint connects to entity_b, not a pin
        let joint = world.get::<ImpulseJoint>(entity_a).unwrap();
        assert_eq!(joint.parent, entity_b);

        // Check visual entity
        let children = world.get::<Children>(entity_a);
        assert!(children.is_some());
        let visual_id = *children
            .unwrap()
            .iter()
            .find(|&&child| world.get::<Connector>(child).is_some())
            .unwrap();

        // Check NO pin entity created
        assert!(cmd.pin_entity.is_none());

        // Undo
        cmd.undo(&mut world);

        // Check ImpulseJoint removed
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());

        // Check visual entity despawned
        assert!(world.get_entity(visual_id).is_err());
    }

    #[rstest]
    fn test_spawn_ground_command(mut world: World) {
        let mut cmd = SpawnGroundCommand {
            position: Vec2::new(10.0, 10.0),
            rotation: 0.0,
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<GroundPlane>(entity).is_some());
        assert!(world.get::<Transform>(entity).is_some());
        assert!(world.get::<Mesh3d>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
    }
}
