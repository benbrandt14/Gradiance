//! Command pattern implementation for Undo/Redo.
//!
//! Defines the `GameCommand` trait and the `CommandStack` resource.

use crate::input::ZIndex;
use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::tools::connector::Connector;
use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::rapier::geometry::SharedShape;
use nalgebra::Point2;

/// A trait for game commands that support Undo/Redo.
pub trait GameCommand: Send + Sync {
    /// Apply the command to the world.
    fn apply(&mut self, world: &mut World);

    /// Revert the command's effects.
    fn undo(&mut self, world: &mut World);
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

        command.apply(world);
        self.history.push(command);
        self.index += 1;
    }

    /// Undoes the last command.
    pub fn undo(&mut self, world: &mut World) {
        if self.index > 0 {
            self.index -= 1;
            if let Some(command) = self.history.get_mut(self.index) {
                command.undo(world);
            }
        }
    }

    /// Redoes the previously undone command.
    pub fn redo(&mut self, world: &mut World) {
        if self.index < self.history.len() {
            if let Some(command) = self.history.get_mut(self.index) {
                command.apply(world);
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
    fn apply(&mut self, world: &mut World) {
        let z = world.resource_mut::<ZIndex>().next();

        let shape = shapes::Rectangle {
            extents: Vec2::new(self.width, self.height),
            origin: shapes::RectangleOrigin::Center,
            ..default()
        };

        let entity = world
            .spawn((
                ShapeBundle {
                    path: GeometryBuilder::build_as(&shape),
                    transform: Transform::from_xyz(self.position.x, self.position.y, z),
                    ..default()
                },
                Fill::color(Color::srgb(0.5, 0.5, 1.0)),
                Stroke::new(Color::BLACK, 0.1),
                RigidBody::Dynamic,
                // Rapier uses half-extents
                Collider::cuboid(self.width / 2.0, self.height / 2.0),
                EditableBox {
                    width: self.width as f64,
                    height: self.height as f64,
                },
                // PickableBundle::default(), // Picking disabled due to incompatibility
            ))
            .id();

        self.entity = Some(entity);
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
    fn apply(&mut self, world: &mut World) {
        let z = world.resource_mut::<ZIndex>().next();

        let shape = shapes::Circle {
            radius: self.radius,
            center: Vec2::ZERO,
        };

        let id = world
            .spawn((
                ShapeBundle {
                    path: GeometryBuilder::build_as(&shape),
                    transform: Transform::from_xyz(self.position.x, self.position.y, z),
                    ..default()
                },
                Fill::color(Color::srgb(1.0, 0.5, 0.5)),
                Stroke::new(Color::BLACK, 0.1),
                RigidBody::Dynamic,
                Collider::ball(self.radius),
                EditableCircle {
                    radius: self.radius as f64,
                },
                // PickableBundle::default(),
            ))
            .id();

        self.entity = Some(id);
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
    fn apply(&mut self, world: &mut World) {
        let z = world.resource_mut::<ZIndex>().next();

        let shape = shapes::Polygon {
            points: self.vertices.clone(),
            closed: true,
        };

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
                ShapeBundle {
                    path: GeometryBuilder::build_as(&shape),
                    transform: Transform::from_xyz(self.position.x, self.position.y, z),
                    ..default()
                },
                Fill::color(Color::srgb(0.5, 1.0, 0.5)),
                Stroke::new(Color::BLACK, 0.1),
                RigidBody::Dynamic,
                collider,
                // PickableBundle::default(),
            ))
            .id();

        self.entity = Some(id);
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
    fn apply(&mut self, world: &mut World) {
        // Visual
        let visual_id = world
            .spawn((
                Transform::from_xyz(self.anchor_a.x, self.anchor_a.y, 0.1),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
                Collider::ball(0.5),
                Sensor,
                Connector {
                    entity_a: self.entity_a,
                    entity_b: self.entity_b,
                    local_anchor_a: self.anchor_a,
                    local_anchor_b: self.anchor_b,
                },
            ))
            .set_parent_in_place(self.entity_a)
            .id();

        let circle_outer = GeometryBuilder::build_as(&shapes::Circle { radius: 5.0, ..default() });
        world.entity_mut(visual_id).insert((
            ShapeBundle { path: circle_outer, ..default() },
            Fill::color(Color::BLACK)
        ));

        let circle_inner = GeometryBuilder::build_as(&shapes::Circle { radius: 2.0, ..default() });
        let inner = world.spawn((
             ShapeBundle {
                 path: circle_inner,
                 transform: Transform::from_translation(Vec3::Z * 0.1),
                 ..default()
             },
             Fill::color(Color::WHITE),
        )).id();
        world.entity_mut(visual_id).add_child(inner);

        self.visual_entity = Some(visual_id);

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

            if let Some(visual_id) = self.visual_entity {
                if let Some(mut connector) = world.get_mut::<Connector>(visual_id) {
                    connector.entity_b = Some(pin_id);
                }
            }

            joint_data = RevoluteJointBuilder::new()
                .local_anchor1(self.anchor_a)
                .local_anchor2(Vec2::ZERO);
        }

        // Attach ImpulseJoint to entity_a
        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
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
    fn apply(&mut self, world: &mut World) {
        let visual_id = world
            .spawn((
                Transform::from_xyz(self.anchor_a.x, self.anchor_a.y, 0.1),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
                Collider::ball(0.5),
                Sensor,
                Connector {
                    entity_a: self.entity_a,
                    entity_b: self.entity_b,
                    local_anchor_a: self.anchor_a,
                    local_anchor_b: self.anchor_b,
                },
            ))
            .set_parent_in_place(self.entity_a)
            .id();

        let line1 = GeometryBuilder::build_as(&shapes::Line(Vec2::new(-3.0, -3.0), Vec2::new(3.0, 3.0)));
        let v1 = world.spawn((
                ShapeBundle {
                    path: line1,
                    transform: Transform::from_translation(Vec3::Z * 0.1),
                    ..default()
                },
                Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
        )).id();

        let line2 = GeometryBuilder::build_as(&shapes::Line(Vec2::new(-3.0, 3.0), Vec2::new(3.0, -3.0)));
        let v2 = world.spawn((
                ShapeBundle {
                    path: line2,
                    transform: Transform::from_translation(Vec3::Z * 0.1),
                    ..default()
                },
                Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
        )).id();
        world.entity_mut(visual_id).add_children(&[v1, v2]);

        self.visual_entity = Some(visual_id);

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

            if let Some(visual_id) = self.visual_entity {
                if let Some(mut connector) = world.get_mut::<Connector>(visual_id) {
                    connector.entity_b = Some(pin_id);
                }
            }

            joint_data = FixedJointBuilder::new()
                .local_anchor1(self.anchor_a)
                .local_anchor2(Vec2::ZERO);
        }

        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
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
