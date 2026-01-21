//! Command pattern implementation for Undo/Redo.
//!
//! Defines the `GameCommand` trait and the `CommandStack` resource.

use crate::input::ZIndex;
use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::tools::connector::Connector;
use crate::physics::floor::GroundPlane;
use crate::physics::layers::PhysicsLayers;
use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::dynamics::TypedJoint;
use bevy_rapier2d::rapier::dynamics::{
    FixedJointBuilder as RapierFixedJointBuilder, GenericJoint,
    RevoluteJointBuilder as RapierRevoluteJointBuilder,
};
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

/// Helper to spawn a shape entity with common components.
fn spawn_shape_entity(
    world: &mut World,
    position: Vec2,
    shape: &impl bevy_prototype_lyon::geometry::Geometry,
    collider: Collider,
    fill: Fill,
    extra_bundle: impl Bundle,
) -> Entity {
    let z = world.resource_mut::<ZIndex>().next();

    world
        .spawn((
            ShapeBundle {
                path: GeometryBuilder::build_as(shape),
                transform: Transform::from_xyz(position.x, position.y, z),
                ..default()
            },
            fill,
            Stroke::new(Color::BLACK, 0.1),
            RigidBody::Dynamic,
            collider,
            PhysicsLayers(1), // Default Layer A
            extra_bundle,
        ))
        .id()
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
            .spawn((RigidBody::Fixed, Transform::from_translation(world_pos)))
            .id();

        pin_entity = Some(pin_id);
        target_entity = pin_id;
        local_anchor_2 = Vec2::ZERO;

        if let Some(v_id) = visual_entity
            && let Some(mut connector) = world.get_mut::<Connector>(v_id) {
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
            Collider::ball(0.5),
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
        let shape = shapes::Rectangle {
            extents: Vec2::new(self.width, self.height),
            origin: shapes::RectangleOrigin::Center,
            radii: None,
        };

        let entity = spawn_shape_entity(
            world,
            self.position,
            &shape,
            Collider::cuboid(self.width / 2.0, self.height / 2.0),
            Fill::color(Color::srgb(0.5, 0.5, 1.0)),
            EditableBox {
                width: self.width as f64,
                height: self.height as f64,
            },
        );

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
        let shape = shapes::Circle {
            radius: self.radius,
            center: Vec2::ZERO,
        };

        let entity = spawn_shape_entity(
            world,
            self.position,
            &shape,
            Collider::ball(self.radius),
            Fill {
                color: Color::srgb(1.0, 0.5, 0.5),
                options: FillOptions::default().with_tolerance(0.001),
            },
            EditableCircle {
                radius: self.radius as f64,
            },
        );

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

        let shape = shapes::Polygon {
            points: self.vertices.clone(),
            closed: true,
        };

        let vertices: Vec<Point2<f32>> = self
            .vertices
            .iter()
            .map(|v| Point2::new(v.x, v.y))
            .collect();
        let indices: Vec<[u32; 2]> = (0..vertices.len())
            .map(|i| [i as u32, ((i + 1) % vertices.len()) as u32])
            .collect();

        let rapier_shape = SharedShape::convex_decomposition(&vertices, &indices);
        let collider = Collider::from(rapier_shape);

        let entity = spawn_shape_entity(
            world,
            self.position,
            &shape,
            collider,
            Fill::color(Color::srgb(0.5, 1.0, 0.5)),
            (),
        );

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
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID (if pinning to world).
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnJointCommand {
    fn name(&self) -> String {
        "Spawn Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        // Visual
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                let circle_outer = GeometryBuilder::build_as(&shapes::Circle {
                    radius: 5.0,
                    ..default()
                });
                world.entity_mut(visual_id).insert((
                    ShapeBundle {
                        path: circle_outer,
                        ..default()
                    },
                    Fill::color(Color::BLACK),
                ));

                let circle_inner = GeometryBuilder::build_as(&shapes::Circle {
                    radius: 2.0,
                    ..default()
                });
                let inner = world
                    .spawn((
                        ShapeBundle {
                            path: circle_inner,
                            transform: Transform::from_translation(Vec3::Z * 0.1),
                            ..default()
                        },
                        Fill::color(Color::WHITE),
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

        let builder = RapierRevoluteJointBuilder::new()
            .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
            .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
        let mut joint_data = GenericJoint::from(builder);
        joint_data.set_contacts_enabled(false);

        // Attach ImpulseJoint to entity_a
        world.entity_mut(self.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }

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

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                let line1 = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-3.0, -3.0),
                    Vec2::new(3.0, 3.0),
                ));
                let v1 = world
                    .spawn((
                        ShapeBundle {
                            path: line1,
                            transform: Transform::from_translation(Vec3::Z * 0.1),
                            ..default()
                        },
                        Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
                    ))
                    .id();

                let line2 = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-3.0, 3.0),
                    Vec2::new(3.0, -3.0),
                ));
                let v2 = world
                    .spawn((
                        ShapeBundle {
                            path: line2,
                            transform: Transform::from_translation(Vec3::Z * 0.1),
                            ..default()
                        },
                        Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
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

        let builder = RapierFixedJointBuilder::new()
            .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
            .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
        let mut joint_data = GenericJoint::from(builder);
        joint_data.set_contacts_enabled(false);

        world.entity_mut(self.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
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

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let width = 100_000.0;
        let depth = 1000.0;

        // Visual shape: Huge rectangle
        let shape = shapes::Rectangle {
            extents: Vec2::new(width, depth),
            origin: shapes::RectangleOrigin::Center,
            radii: None,
        };

        // Offset: the visual rectangle is centered at (0,0).
        // The collider is centered at (0,0).
        // But we want the "surface" (top edge) to be at `position` aligned with `rotation`.
        // The box center is at (0, -depth/2) relative to the surface.
        // So we rotate (0, -depth/2) by `rotation` and add to `position`.

        let rot = Quat::from_rotation_z(self.rotation);
        let offset = rot * Vec3::new(0.0, -depth / 2.0, 0.0);
        let center = Vec3::new(self.position.x, self.position.y, 0.0) + offset;

        let z = -1.0; // Force ground to be behind

        let entity = world
            .spawn((
                // Shape Bundle
                ShapeBundle {
                    path: GeometryBuilder::build_as(&shape),
                    transform: Transform::from_translation(center + Vec3::new(0.0, 0.0, z))
                        .with_rotation(rot),
                    ..default()
                },
                Fill::color(Color::srgb(0.2, 0.2, 0.2)), // Dark grey
                Stroke::new(Color::BLACK, 2.0),
                // Physics
                RigidBody::Fixed,
                Collider::cuboid(width / 2.0, depth / 2.0),
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
        world
    }

    #[rstest]
    fn test_spawn_polygon_command_failure(mut world: World) {
        let vertices = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]; // Only 2 vertices
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
        assert_eq!(
            transform.unwrap().translation.truncate(),
            Vec2::new(10.0, 20.0)
        );

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
        assert_eq!(
            transform.unwrap().translation.truncate(),
            Vec2::new(-5.0, 5.0)
        );

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
        // Verify shape bundle exists (checking Transform as proxy)
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

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
    }
}
