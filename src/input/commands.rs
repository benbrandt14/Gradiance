//! Command pattern implementation for Undo/Redo.
//!
//! Defines the [`GameCommand`] trait and the [`CommandStack`] resource.

use crate::geometry::extrusion::ExtrudableShape;
use crate::input::editable_shape::{EditableShape, ShapeType, generate_shape_components};
use crate::input::tools::connector::Connector;
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use crate::prelude::EntityId;
use anyhow::{Result, bail};
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::dynamics::{GenericJoint as BevyGenericJoint, TypedJoint};
use bevy_rapier2d::rapier::dynamics::{GenericJoint as RapierGenericJoint};
use std::fmt::Debug;

const GROUND_WIDTH: f32 = 100_000.0;
const GROUND_DEPTH: f32 = 1000.0;
const CONNECTOR_COLLIDER_RADIUS: f32 = 0.5;
const VISUAL_CIRCLE_OUTER_RADIUS: f32 = 5.0;
const VISUAL_LINE_OFFSET: f32 = 3.0;
const PIN_GROUP: Group = Group::GROUP_32;

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

    /// Pushes a command that has already been applied.
    /// Clears any redo history.
    pub fn push_without_apply(&mut self, command: Box<dyn GameCommand>) {
        if self.index < self.history.len() {
            self.history.truncate(self.index);
        }
        self.history.push(command);
        self.index += 1;
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
    entity_a: EntityId,
    entity_b: Option<EntityId>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    visual_entity: Option<EntityId>,
    pin_solver_groups: SolverGroups,
) -> (Entity, Option<Entity>, Vec2, Vec2) {
    let mut pin_entity = None;
    let target_entity;
    let local_anchor_1 = anchor_a;
    let local_anchor_2;

    let e_a = entity_a.0;

    if let Some(e_b_id) = entity_b {
        let e_b = e_b_id.0;
        target_entity = e_b;
        local_anchor_2 = anchor_b;
    } else {
        // Pin logic
        let t_a = world
            .get::<GlobalTransform>(e_a)
            .map(|t| t.compute_transform())
            .unwrap_or_default();
        let world_pos = t_a.transform_point(Vec3::new(anchor_a.x, anchor_a.y, 0.0));

        let pin_id = world
            .spawn((
                RigidBody::Fixed,
                Transform::from_translation(world_pos),
                Collider::ball(CONNECTOR_COLLIDER_RADIUS),
                pin_solver_groups,
                StateScoped(GameState::Playing),
            ))
            .id();

        pin_entity = Some(pin_id);
        target_entity = pin_id;
        local_anchor_2 = Vec2::ZERO;

        if let Some(v_id) = visual_entity
            && let Some(mut connector) = world.get_mut::<Connector>(v_id.0)
        {
            connector.entity_b = Some(pin_id);
        }
    }

    (target_entity, pin_entity, local_anchor_1, local_anchor_2)
}

/// Helper to spawn connector visual.
fn spawn_connector_visual(
    world: &mut World,
    entity_a: EntityId,
    entity_b: Option<EntityId>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    children_builder: impl FnOnce(&mut World, Entity),
) -> EntityId {
    let visual_id = world
        .spawn((
            Transform::from_xyz(anchor_a.x, anchor_a.y, 0.1),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Collider::ball(CONNECTOR_COLLIDER_RADIUS),
            Sensor,
            Connector {
                entity_a: entity_a.0,
                entity_b: entity_b.map(|e| e.0),
                local_anchor_a: anchor_a,
                local_anchor_b: anchor_b,
            },
            StateScoped(GameState::Playing),
        ))
        .set_parent_in_place(entity_a.0)
        .id();

    children_builder(world, visual_id);

    EntityId(visual_id)
}

/// Command to spawn a generic shape.
#[derive(Debug)]
pub struct SpawnShapeCommand {
    /// Position of the shape.
    pub position: Vec2,
    /// The shape definition.
    pub shape: ShapeType,
    /// The spawned entity ID.
    pub entity: Option<EntityId>,
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

        let z = 0.0;
        let name = match &self.shape {
            ShapeType::Box { .. } => "Box",
            ShapeType::Circle { .. } => "Circle",
            ShapeType::Polygon { .. } => "Polygon",
        };

        let entity = world
            .spawn((
                path,
                Transform::from_xyz(self.position.x, self.position.y, z),
                Visibility::default(),
                RigidBody::Dynamic,
                collider,
                CollisionGroups::default(),
                EditableShape {
                    shape: self.shape.clone(),
                },
                ExtrudableShape,
                Name::new(name.to_string()),
                StateScoped(GameState::Playing),
            ))
            .id();

        self.entity = Some(EntityId(entity));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(entity) = self.entity {
            if let Ok(entity_ref) = world.get_entity_mut(entity.0) {
                entity_ref.despawn();
            }
            self.entity = None;
        }
    }
}

// TODO(TECH_DEBT.md): Code Duplication in Spawn Commands - Extract common logic into a shared helper or builder pattern.
/// Command to spawn a Revolute Joint (Hinge).
#[derive(Debug)]
pub struct SpawnJointCommand {
    /// The first body.
    pub entity_a: EntityId,
    /// The second body (optional).
    pub entity_b: Option<EntityId>,
    /// Anchor on body A.
    pub anchor_a: Vec2,
    /// Anchor on body B.
    pub anchor_b: Vec2,
    /// Joint compliance.
    pub compliance: f32,
    /// The visual entity ID.
    pub visual_entity: Option<EntityId>,
    /// The pin entity ID (if pinning to world).
    pub pin_entity: Option<EntityId>,
    /// Previous solver groups of the pinned body.
    pub original_solver_groups: Option<SolverGroups>,
}

// TODO(TECH_DEBT.md): Pin Collision Instability - Explicitly filter at the broadphase level.
impl GameCommand for SpawnJointCommand {
    fn name(&self) -> String {
        "Spawn Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
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

        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a.0).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a.0).insert(new_groups);
        }

        let pin_groups = SolverGroups::new(PIN_GROUP, Group::ALL);
        // TODO: Implement CollisionGroups filtering for the pin entity.
        // Currently, we only modify SolverGroups, which disables contact resolution but not broadphase intersection.
        // This can still cause instability or explosions in some Rapier configurations.
        // Add `CollisionGroups::new(PIN_GROUP, Group::ALL)` (or similar filter) to the pin spawn logic
        // in `resolve_joint_targets` or here.

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
            pin_groups,
        );
        self.pin_entity = pin_entity.map(EntityId);

        let joint_data = RevoluteJointBuilder::new()
            .local_anchor1(local_anchor_1)
            .local_anchor2(local_anchor_2);

        let typed: TypedJoint = joint_data.into();
        let mut rapier_generic: RapierGenericJoint = match typed {
            TypedJoint::RevoluteJoint(wrapper) => wrapper.data.raw,
            _ => unreachable!(),
        };
        rapier_generic.set_contacts_enabled(false);
        let bevy_generic = BevyGenericJoint { raw: rapier_generic };

        world
            .entity_mut(self.entity_a.0)
            .insert(ImpulseJoint::new(target_entity, TypedJoint::GenericJoint(bevy_generic)));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v.0) {
                e.despawn();
            }
            self.visual_entity = None;
        }

        if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
            e.remove::<ImpulseJoint>();
        }

        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p.0) {
                e.despawn();
            }
            self.pin_entity = None;
        }

        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                e.remove::<SolverGroups>();
            }
        }
    }
}

/// Command to spawn a Prismatic Joint (Slider).
#[derive(Debug)]
pub struct SpawnPrismaticJointCommand {
    /// The first body.
    pub entity_a: EntityId,
    /// The second body (optional).
    pub entity_b: Option<EntityId>,
    /// Anchor on body A.
    pub anchor_a: Vec2,
    /// Anchor on body B.
    pub anchor_b: Vec2,
    /// Axis of movement.
    pub axis: Vec2,
    /// Joint compliance.
    pub compliance: f32,
    /// The visual entity ID.
    pub visual_entity: Option<EntityId>,
    /// The pin entity ID.
    pub pin_entity: Option<EntityId>,
    /// Previous solver groups of the pinned body.
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
                let line = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-VISUAL_LINE_OFFSET * 2.0, 0.0),
                    Vec2::new(VISUAL_LINE_OFFSET * 2.0, 0.0),
                ));
                world.entity_mut(visual_id).insert(path_from_shape(line));
            },
        );
        self.visual_entity = Some(visual_id);

        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a.0).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a.0).insert(new_groups);
        }

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
        self.pin_entity = pin_entity.map(EntityId);

        let joint_data = PrismaticJointBuilder::new(self.axis)
            .local_anchor1(local_anchor_1)
            .local_anchor2(local_anchor_2);

        let typed: TypedJoint = joint_data.into();
        let mut rapier_generic: RapierGenericJoint = match typed {
            TypedJoint::PrismaticJoint(wrapper) => wrapper.data.raw,
            _ => unreachable!(),
        };
        rapier_generic.set_contacts_enabled(false);
        let bevy_generic = BevyGenericJoint { raw: rapier_generic };

        world
            .entity_mut(self.entity_a.0)
            .insert(ImpulseJoint::new(target_entity, TypedJoint::GenericJoint(bevy_generic)));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v.0) {
                e.despawn();
            }
            self.visual_entity = None;
        }

        if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
            e.remove::<ImpulseJoint>();
        }

        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p.0) {
                e.despawn();
            }
            self.pin_entity = None;
        }

        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                e.remove::<SolverGroups>();
            }
        }
    }
}

/// Command to spawn a Fixed Joint (Weld).
#[derive(Debug)]
pub struct SpawnFixedJointCommand {
    /// The first body.
    pub entity_a: EntityId,
    /// The second body (optional).
    pub entity_b: Option<EntityId>,
    /// Anchor on body A.
    pub anchor_a: Vec2,
    /// Anchor on body B.
    pub anchor_b: Vec2,
    /// Joint compliance.
    pub compliance: f32,
    /// The visual entity ID.
    pub visual_entity: Option<EntityId>,
    /// The pin entity ID.
    pub pin_entity: Option<EntityId>,
    /// Previous solver groups of the pinned body.
    pub original_solver_groups: Option<SolverGroups>,
    /// Rotation of body A.
    pub rot_a: f32,
    /// Rotation of body B.
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
                let line1 = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-VISUAL_LINE_OFFSET, -VISUAL_LINE_OFFSET),
                    Vec2::new(VISUAL_LINE_OFFSET, VISUAL_LINE_OFFSET),
                ));
                world.entity_mut(visual_id).insert(path_from_shape(line1));
            },
        );
        self.visual_entity = Some(visual_id);

        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a.0).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a.0).insert(new_groups);
        }

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
        self.pin_entity = pin_entity.map(EntityId);

        let joint_data = FixedJointBuilder::new()
            .local_anchor1(local_anchor_1)
            .local_anchor2(local_anchor_2)
            .local_basis1(-self.rot_a)
            .local_basis2(-self.rot_b);

        let typed: TypedJoint = joint_data.into();
        let mut rapier_generic: RapierGenericJoint = match typed {
            TypedJoint::FixedJoint(wrapper) => wrapper.data.raw,
            _ => unreachable!(),
        };
        rapier_generic.set_contacts_enabled(false);
        let bevy_generic = BevyGenericJoint { raw: rapier_generic };

        world
            .entity_mut(self.entity_a.0)
            .insert(ImpulseJoint::new(target_entity, TypedJoint::GenericJoint(bevy_generic)));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v.0) {
                e.despawn();
            }
            self.visual_entity = None;
        }

        if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
            e.remove::<ImpulseJoint>();
        }

        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p.0) {
                e.despawn();
            }
            self.pin_entity = None;
        }

        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                e.remove::<SolverGroups>();
            }
        }
    }
}

/// Command to spawn an infinite ground plane.
#[derive(Debug)]
pub struct SpawnGroundCommand {
    /// Position of the ground.
    pub position: Vec2,
    /// Rotation angle.
    pub rotation: f32,
    /// The spawned entity ID.
    pub entity: Option<EntityId>,
}

impl GameCommand for SpawnGroundCommand {
    fn name(&self) -> String {
        "Spawn Ground".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
        let shape = shapes::Rectangle {
            extents: Vec2::new(GROUND_WIDTH, GROUND_DEPTH),
            origin: shapes::RectangleOrigin::Center,
            radii: None,
        };

        let rot = Quat::from_rotation_z(self.rotation);
        let offset = rot * Vec3::new(0.0, -GROUND_DEPTH / 2.0, 0.0);
        let center = Vec3::new(self.position.x, self.position.y, 0.0) + offset;

        let z = 0.0;

        let entity = world
            .spawn((
                GeometryBuilder::build_as(&shape),
                Transform::from_translation(center + Vec3::new(0.0, 0.0, z)).with_rotation(rot),
                Visibility::default(),
                RigidBody::Fixed,
                Collider::cuboid(GROUND_WIDTH / 2.0, GROUND_DEPTH / 2.0),
                Friction::coefficient(0.5),
                Restitution::coefficient(0.0),
                CollisionGroups::new(Group::ALL, Group::ALL),
                GroundPlane,
                Name::new("Ground"),
                ExtrudableShape,
                StateScoped(GameState::Playing),
            ))
            .id();

        self.entity = Some(EntityId(entity));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(e) = self.entity {
            if let Ok(e_ref) = world.get_entity_mut(e.0) {
                e_ref.despawn();
            }
            self.entity = None;
        }
    }
}

/// Command to update transform of entities (Move/Rotate).
#[derive(Debug)]
pub struct MoveEntitiesCommand {
    /// List of entities and their (Old, New) positions.
    pub position_changes: Vec<(EntityId, Vec2, Vec2)>,
    /// List of entities and their (Old, New) rotations.
    pub rotation_changes: Vec<(EntityId, Quat, Quat)>,
}

impl GameCommand for MoveEntitiesCommand {
    fn name(&self) -> String {
        "Move Entities".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
        for (id, _, new_pos) in &self.position_changes {
            if let Some(mut t) = world.get_mut::<Transform>(id.0) {
                t.translation.x = new_pos.x;
                t.translation.y = new_pos.y;
            }
        }
        for (id, _, new_rot) in &self.rotation_changes {
            if let Some(mut t) = world.get_mut::<Transform>(id.0) {
                t.rotation = *new_rot;
            }
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        for (id, old_pos, _) in &self.position_changes {
            if let Some(mut t) = world.get_mut::<Transform>(id.0) {
                t.translation.x = old_pos.x;
                t.translation.y = old_pos.y;
            }
        }
        for (id, old_rot, _) in &self.rotation_changes {
            if let Some(mut t) = world.get_mut::<Transform>(id.0) {
                t.rotation = *old_rot;
            }
        }
    }
}

/// Command to duplicate entities.
#[derive(Debug)]
pub struct DuplicateEntitiesCommand {
    /// The source entities to duplicate.
    pub source_entities: Vec<EntityId>,
    /// The spawned entities (recorded during apply).
    pub created_entities: Vec<EntityId>,
    /// The offset to apply to duplicates.
    pub offset: Vec2,
    /// Whether to spawn them as kinematic.
    pub make_kinematic: bool,
}

impl GameCommand for DuplicateEntitiesCommand {
    fn name(&self) -> String {
        "Duplicate Entities".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<()> {
        self.created_entities.clear();

        let mut new_ids = Vec::new();

        for source_id in &self.source_entities {
            let src = source_id.0;

            if world.get_entity(src).is_err() {
                continue;
            }

            let transform = world.get::<Transform>(src).copied();
            let collider = world.get::<Collider>(src).cloned();
            let rb = world.get::<RigidBody>(src).copied();
            let eshape = world.get::<EditableShape>(src).cloned();
            let fill = world.get::<Fill>(src).copied();
            let stroke = world.get::<Stroke>(src).copied();
            let friction = world.get::<Friction>(src).copied();
            let restitution = world.get::<Restitution>(src).copied();
            let density = world.get::<ColliderMassProperties>(src).cloned();
            let gravity = world.get::<GravityScale>(src).copied();
            let locked = world.get::<LockedAxes>(src).copied();
            let sensor = world.get::<Sensor>(src).copied();
            let groups = world.get::<CollisionGroups>(src).copied();

            if let Some(t) = transform {
                let mut builder = world.spawn(t);

                let mut new_t = t;
                new_t.translation += Vec3::new(self.offset.x, self.offset.y, 0.0);
                builder.insert(new_t);

                if let Some(c) = collider { builder.insert(c); }

                if self.make_kinematic {
                    builder.insert(RigidBody::KinematicPositionBased);
                } else if let Some(c) = rb {
                    builder.insert(c);
                }

                if let Some(c) = eshape { builder.insert(c); }
                if let Some(c) = fill { builder.insert(c); }
                if let Some(c) = stroke { builder.insert(c); }
                if let Some(c) = friction { builder.insert(c); }
                if let Some(c) = restitution { builder.insert(c); }
                if let Some(c) = density { builder.insert(c); }
                if let Some(c) = gravity { builder.insert(c); }
                if let Some(c) = locked { builder.insert(c); }
                if let Some(c) = sensor { builder.insert(c); }
                if let Some(c) = groups { builder.insert(c); }

                builder.insert(Name::new("Duplicated Entity"));
                builder.insert(StateScoped(GameState::Playing));
                builder.insert(ExtrudableShape);

                new_ids.push(EntityId(builder.id()));
            }
        }

        self.created_entities = new_ids;
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        for id in &self.created_entities {
            if let Ok(e) = world.get_entity_mut(id.0) {
                e.despawn();
            }
        }
        self.created_entities.clear();
    }
}

// Helpers
fn path_from_shape(path: Path) -> Path {
    path
}
