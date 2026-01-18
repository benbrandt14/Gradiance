//! Command pattern implementation for Undo/Redo.
//!
//! Defines the `GameCommand` trait and the `CommandStack` resource.

use crate::prelude::*;
use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::tools::connector::Connector;
use crate::input::ZIndex;
use avian2d::prelude::*;
use bevy::math::DVec2;
use bevy_prototype_lyon::prelude::*;
use bevy_prototype_lyon::prelude::tess::{
    FillOptions, VertexBuffers,
    geometry_builder::simple_builder,
    math::Point,
    FillTessellator,
    path::Path,
};

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

        let geometry = ShapeBuilder::with(&shape)
            .fill(Color::srgb(0.5, 0.5, 1.0))
            .stroke(Stroke::new(Color::BLACK, 0.1))
            .build();

        let entity = world.spawn((
            geometry,
            RigidBody::Dynamic,
            Collider::rectangle(self.width as f64, self.height as f64),
            EditableBox {
                width: self.width as f64,
                height: self.height as f64,
            },
            Transform::from_xyz(self.position.x, self.position.y, z),
        )).id();

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

        let geometry = ShapeBuilder::with(&shape)
            .fill(Color::srgb(1.0, 0.5, 0.5))
            .stroke(Stroke::new(Color::BLACK, 0.1))
            .build();

        let id = world.spawn((
            geometry,
            RigidBody::Dynamic,
            Collider::circle(self.radius as f64),
            EditableCircle { radius: self.radius as f64 },
            Transform::from_xyz(self.position.x, self.position.y, z),
        )).id();

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

        let geometry = ShapeBuilder::with(&shape)
            .fill(Color::srgb(0.5, 1.0, 0.5))
            .stroke(Stroke::new(Color::BLACK, 0.1))
            .build();

        // Tessellation logic for collider
        let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
        let mut vertex_builder = simple_builder(&mut buffers);
        let mut tessellator = FillTessellator::new();
        let options = FillOptions::default();

        let lyon_points: Vec<Point> = self.vertices.iter()
            .map(|p| Point::new(p.x, p.y))
            .collect();

        let mut path_builder = Path::builder();
        if let Some(first) = lyon_points.first() {
            path_builder.begin(*first);
            for p in lyon_points.iter().skip(1) {
                path_builder.line_to(*p);
            }
            path_builder.close();
        }
        let path = path_builder.build();

        let collider = if tessellator.tessellate_path(
                &path,
                &options,
                &mut vertex_builder
            ).is_ok() {
            let vertices: Vec<DVec2> = buffers.vertices.iter()
                .map(|p| DVec2::new(p.x as f64, p.y as f64))
                .collect();

            let indices: Vec<[u32; 3]> = buffers.indices.chunks(3)
                .map(|c| [c[0] as u32, c[1] as u32, c[2] as u32])
                .collect();

            Collider::trimesh(vertices, indices)
        } else {
             let dvec_points: Vec<DVec2> = self.vertices.iter().map(|p| DVec2::new(p.x as f64, p.y as f64)).collect();
             Collider::convex_hull(dvec_points).unwrap_or(Collider::circle(1.0))
        };

        let id = world.spawn((
            geometry,
            RigidBody::Dynamic,
            collider,
            Transform::from_xyz(self.position.x, self.position.y, z),
        )).id();

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
    pub anchor_a: DVec2,
    /// Anchor on body B (local).
    pub anchor_b: DVec2,
    /// Joint compliance (inverse stiffness).
    pub compliance: f64,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID (if pinning to world).
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnJointCommand {
    fn apply(&mut self, world: &mut World) {
        let visual_id = world.spawn((
            Transform::from_xyz(self.anchor_a.x as f32, self.anchor_a.y as f32, 0.1),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Collider::circle(0.5),
            Sensor,
            Connector {
                entity_a: self.entity_a,
                entity_b: self.entity_b,
                local_anchor_a: self.anchor_a,
                local_anchor_b: self.anchor_b,
            },
        )).set_parent_in_place(self.entity_a).id();

        let circle = ShapeBuilder::with(&shapes::Circle { radius: 5.0, ..default() }).fill(Color::BLACK).build();
        world.entity_mut(visual_id).insert(circle);
        let inner = world.spawn((
             ShapeBuilder::with(&shapes::Circle { radius: 2.0, ..default() }).fill(Color::WHITE).build(),
             Transform::from_translation(Vec3::Z * 0.1),
        )).id();
        world.entity_mut(visual_id).add_child(inner);

        self.visual_entity = Some(visual_id);

        if let Some(e_b) = self.entity_b {
            world.entity_mut(self.entity_a).insert(
                RevoluteJoint::new(self.entity_a, e_b)
                    .with_local_anchor1(self.anchor_a)
                    .with_local_anchor2(self.anchor_b)
                    .with_point_compliance(self.compliance)
            );
        } else {
             // Pin logic
            let t_a = world.get::<GlobalTransform>(self.entity_a).map(|t| t.compute_transform()).unwrap_or_default();
            let world_pos = t_a.transform_point(Vec3::new(self.anchor_a.x as f32, self.anchor_a.y as f32, 0.0));

            let pin_id = world.spawn((
                RigidBody::Static,
                Transform::from_translation(world_pos),
            )).id();

            self.pin_entity = Some(pin_id);

            if let Some(visual_id) = self.visual_entity {
                if let Ok(mut connector) = world.get_mut::<Connector>(visual_id) {
                    connector.entity_b = Some(pin_id);
                }
            }

            world.entity_mut(self.entity_a).insert(
                 RevoluteJoint::new(self.entity_a, pin_id)
                    .with_local_anchor1(self.anchor_a)
                    .with_local_anchor2(DVec2::ZERO)
                    .with_point_compliance(self.compliance)
            );
        }
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
             if let Ok(e) = world.get_entity_mut(v) { e.despawn(); }
             self.visual_entity = None;
        }

        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<RevoluteJoint>();
        }

        if let Some(p) = self.pin_entity {
             if let Ok(e) = world.get_entity_mut(p) { e.despawn(); }
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
    pub anchor_a: DVec2,
    /// Anchor on body B (local).
    pub anchor_b: DVec2,
    /// Joint compliance.
    pub compliance: f64,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID.
    pub pin_entity: Option<Entity>,
    /// Rotation of body A (radians).
    pub rot_a: f64,
    /// Rotation of body B (radians).
    pub rot_b: f64,
}

impl GameCommand for SpawnFixedJointCommand {
    fn apply(&mut self, world: &mut World) {
        let visual_id = world.spawn((
            Transform::from_xyz(self.anchor_a.x as f32, self.anchor_a.y as f32, 0.1),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Collider::circle(0.5),
            Sensor,
            Connector {
                entity_a: self.entity_a,
                entity_b: self.entity_b,
                local_anchor_a: self.anchor_a,
                local_anchor_b: self.anchor_b,
            },
        )).set_parent_in_place(self.entity_a).id();

        let v1 = world.spawn((
                ShapeBuilder::with(&shapes::Line(Vec2::new(-3.0, -3.0), Vec2::new(3.0, 3.0)))
                    .stroke(Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0))
                    .build(),
                Transform::from_translation(Vec3::Z * 0.1),
        )).id();
        let v2 = world.spawn((
                ShapeBuilder::with(&shapes::Line(Vec2::new(-3.0, 3.0), Vec2::new(3.0, -3.0)))
                    .stroke(Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0))
                    .build(),
                Transform::from_translation(Vec3::Z * 0.1),
        )).id();
        world.entity_mut(visual_id).add_children(&[v1, v2]);

        self.visual_entity = Some(visual_id);

        let basis = Rot2::radians((self.rot_a - self.rot_b) as f32);

        if let Some(e_b) = self.entity_b {
             world.entity_mut(self.entity_a).insert(
                FixedJoint::new(self.entity_a, e_b)
                    .with_local_anchor1(self.anchor_a)
                    .with_local_anchor2(self.anchor_b)
                    .with_local_basis2(basis)
                    .with_point_compliance(self.compliance)
            );
        } else {
             let t_a = world.get::<GlobalTransform>(self.entity_a).map(|t| t.compute_transform()).unwrap_or_default();
            let world_pos = t_a.transform_point(Vec3::new(self.anchor_a.x as f32, self.anchor_a.y as f32, 0.0));

            let pin_id = world.spawn((
                RigidBody::Static,
                Transform::from_translation(world_pos),
            )).id();

            self.pin_entity = Some(pin_id);

            if let Some(visual_id) = self.visual_entity {
                if let Ok(mut connector) = world.get_mut::<Connector>(visual_id) {
                    connector.entity_b = Some(pin_id);
                }
            }

            world.entity_mut(self.entity_a).insert(
                FixedJoint::new(self.entity_a, pin_id)
                    .with_local_anchor1(self.anchor_a)
                    .with_local_anchor2(DVec2::ZERO)
                    .with_local_basis2(Rot2::radians(self.rot_a as f32))
                    .with_point_compliance(self.compliance)
            );
        }
    }

    fn undo(&mut self, world: &mut World) {
         if let Some(v) = self.visual_entity {
             if let Ok(e) = world.get_entity_mut(v) { e.despawn(); }
             self.visual_entity = None;
        }

        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<FixedJoint>();
        }

        if let Some(p) = self.pin_entity {
             if let Ok(e) = world.get_entity_mut(p) { e.despawn(); }
             self.pin_entity = None;
        }
    }
}
