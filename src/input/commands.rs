//! Command pattern implementation for Undo/Redo.
//!
//! Defines the [`GameCommand`] trait and the [`CommandStack`] resource.

use crate::geometry::extrusion::ExtrudableShape;
use crate::input::editable_shape::{EditableShape, ShapeType, generate_shape_components};
use crate::input::tools::connector::Connector;
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use anyhow::{Result, bail};
use bevy_prototype_lyon::prelude::*;
use std::fmt::Debug;

const GROUND_WIDTH: f32 = 100_000.0;
const GROUND_DEPTH: f32 = 1000.0;
const CONNECTOR_COLLIDER_RADIUS: f32 = 0.5;
const VISUAL_CIRCLE_OUTER_RADIUS: f32 = 5.0;
const VISUAL_LINE_OFFSET: f32 = 3.0;
const PIN_GROUP: Group = Group::GROUP_32;

// Removed unused colors and stroke width constants to silence warnings

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
// TODO: Use Strong IDs (wrapper types) instead of raw `Entity` for better persistence and safety.
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
    // TODO: Consider using Events (e.g., `EventWriter<CommandEvent>`) to trigger these actions
    // instead of direct resource mutation, to better decouple logic and allow other systems to hook in.
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

    /// Returns the number of commands in history.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Returns the current index in history.
    pub fn current_index(&self) -> usize {
        self.index
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
    pin_solver_groups: SolverGroups,
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
                Collider::ball(CONNECTOR_COLLIDER_RADIUS),
                pin_solver_groups,
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

        // Z-Index is handled by ExtrudableShape based on CollisionGroups now.
        // But we still set 0.0 here.
        let z = 0.0;

        // TODO: Add `Name` component for debugging and `StateScoped` (or cleanup) component for lifecycle management.
        let entity = world
            .spawn((
                path, // Insert Path directly
                Transform::from_xyz(self.position.x, self.position.y, z),
                Visibility::default(), // Required for Mesh3d
                RigidBody::Dynamic,
                collider,
                CollisionGroups::default(), // Default groups
                EditableShape {
                    shape: self.shape.clone(),
                },
                ExtrudableShape, // Trigger extrusion
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
    /// Previous solver groups of the pinned body (for undo).
    pub original_solver_groups: Option<SolverGroups>,
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
                let circle_outer = GeometryBuilder::build_as(&shapes::Circle {
                    radius: VISUAL_CIRCLE_OUTER_RADIUS,
                    ..default()
                });
                world
                    .entity_mut(visual_id)
                    .insert(path_from_shape(circle_outer));
            },
        );
        self.visual_entity = Some(visual_id);

        // Capture original solver groups (Only if pinning to world)
        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a).insert(new_groups);
        }

        // Pin groups
        let pin_groups = SolverGroups::new(PIN_GROUP, Group::ALL);

        // Physics Joint
        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
            pin_groups,
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

        // Restore solver groups
        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
                e.remove::<SolverGroups>();
            }
        }
    }
}

/// Command to spawn a Prismatic Joint (Slider).
#[derive(Debug)]
pub struct SpawnPrismaticJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// Axis of translation (local to body A).
    pub axis: Vec2,
    /// Joint compliance.
    pub compliance: f32,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID.
    pub pin_entity: Option<Entity>,
    /// Previous solver groups of the pinned body (for undo).
    pub original_solver_groups: Option<SolverGroups>,
}

impl GameCommand for SpawnPrismaticJointCommand {
    fn name(&self) -> String {
        "Spawn Prismatic Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                // Visual: A line representing the slider axis?
                // For now, similar to FixedJoint but maybe longer or different color if we had colors.
                let line = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-VISUAL_LINE_OFFSET * 2.0, 0.0),
                    Vec2::new(VISUAL_LINE_OFFSET * 2.0, 0.0),
                ));
                world.entity_mut(visual_id).insert(path_from_shape(line));
            },
        );
        self.visual_entity = Some(visual_id);

        // Capture original solver groups (Only if pinning to world)
        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a).insert(new_groups);
        }

        // Pin groups
        let pin_groups = SolverGroups::new(PIN_GROUP, Group::ALL);

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
            pin_groups,
        );
        self.pin_entity = pin_entity;

        let joint_data = PrismaticJointBuilder::new(self.axis)
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

        // Restore solver groups
        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
                e.remove::<SolverGroups>();
            }
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
    /// Previous solver groups of the pinned body (for undo).
    pub original_solver_groups: Option<SolverGroups>,
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
                // Visual lines removed or need Path?
                // The original code had GeometryBuilder::build_as(&shapes::Line...
                // But we are removing ShapeBundle dependencies.
                // Let's create Paths for lines if we want visual.
                // For now, empty visual logic as per PR cleanup?
                // Or restore using Path component.

                let line1 = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-VISUAL_LINE_OFFSET, -VISUAL_LINE_OFFSET),
                    Vec2::new(VISUAL_LINE_OFFSET, VISUAL_LINE_OFFSET),
                ));
                world.entity_mut(visual_id).insert(path_from_shape(line1));
            },
        );
        self.visual_entity = Some(visual_id);

        // Capture original solver groups (Only if pinning to world)
        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a).insert(new_groups);
        }

        // Pin groups
        let pin_groups = SolverGroups::new(PIN_GROUP, Group::ALL);

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
            pin_groups,
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

        // Restore solver groups
        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
                e.remove::<SolverGroups>();
            }
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
        // Visual shape: Huge rectangle
        let shape = shapes::Rectangle {
            extents: Vec2::new(GROUND_WIDTH, GROUND_DEPTH),
            origin: shapes::RectangleOrigin::Center,
            radii: None,
        };

        // Offset
        let rot = Quat::from_rotation_z(self.rotation);
        let offset = rot * Vec3::new(0.0, -GROUND_DEPTH / 2.0, 0.0);
        let center = Vec3::new(self.position.x, self.position.y, 0.0) + offset;

        let z = 0.0; // Extrusion determines Z

        let entity = world
            .spawn((
                GeometryBuilder::build_as(&shape), // Path
                Transform::from_translation(center + Vec3::new(0.0, 0.0, z)).with_rotation(rot),
                Visibility::default(),
                // Physics
                RigidBody::Fixed,
                Collider::cuboid(GROUND_WIDTH / 2.0, GROUND_DEPTH / 2.0),
                Friction::coefficient(0.5),
                Restitution::coefficient(0.0),
                CollisionGroups::new(Group::ALL, Group::ALL), // Ground covers everything
                GroundPlane,
                Name::new("Ground"),
                ExtrudableShape,
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

// Helpers
fn path_from_shape(path: Path) -> Path {
    path
}
