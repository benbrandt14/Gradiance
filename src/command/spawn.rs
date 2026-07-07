//! Spawn, delete, and duplicate commands.

use crate::command::snapshot::BodyRecord;
use crate::command::{CommandError, GameCommand};
use crate::core::ids::{IdIndex, StableId};
use bevy::prelude::*;

/// Resolves a stable id to its live entity.
fn resolve(world: &World, id: StableId) -> Result<Entity, CommandError> {
    world
        .resource::<IdIndex>()
        .entity(id)
        .ok_or(CommandError::MissingEntity(id))
}

/// Spawns one body from a record.
#[derive(Debug)]
pub struct SpawnBodyCommand {
    /// The authored state to create; its `id` is reused on redo.
    pub record: BodyRecord,
}

impl GameCommand for SpawnBodyCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        self.record.shape.validate()?;
        self.record.spawn(world);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let entity = resolve(world, self.record.id)?;
        world.despawn(entity);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Spawn body"
    }
}

/// Deletes a set of bodies (and every joint referencing them), restoring
/// everything — same ids — on undo.
#[derive(Debug)]
pub struct DeleteCommand {
    /// Bodies to delete.
    pub targets: Vec<StableId>,
    /// Captured body state for undo; filled during `apply`.
    records: Vec<BodyRecord>,
    /// Captured joints (targeted or cascaded); filled during `apply`.
    joint_records: Vec<crate::command::snapshot::JointRecord>,
}

impl DeleteCommand {
    /// Builds a delete command for `targets`.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self {
            targets,
            records: Vec::new(),
            joint_records: Vec::new(),
        }
    }
}

impl GameCommand for DeleteCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        // Capture-first so a missing entity aborts before any despawn.
        let mut pairs = Vec::with_capacity(self.targets.len());
        for &id in &self.targets {
            let entity = resolve(world, id)?;
            let record =
                BodyRecord::capture(world, entity).ok_or(CommandError::MissingEntity(id))?;
            pairs.push((entity, record));
        }
        if pairs.is_empty() {
            return Err(CommandError::NoEffect);
        }
        // Cascade: joints referencing any deleted body go too (a joint
        // with a dangling endpoint must never exist).
        let joints = crate::command::joint_cmd::joints_referencing(world, &self.targets);
        self.records = pairs.iter().map(|(_, r)| r.clone()).collect();
        self.joint_records = joints.iter().map(|(_, r)| r.clone()).collect();
        for (entity, _) in joints {
            world.despawn(entity);
        }
        for (entity, _) in pairs {
            world.despawn(entity);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for record in &self.records {
            record.spawn(world);
        }
        for record in &self.joint_records {
            record.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Delete"
    }
}

/// The next unused selection-group id.
pub(crate) fn next_group_id(world: &mut World) -> u32 {
    let mut query = world.query::<&crate::domain::group::SelectionGroup>();
    query
        .iter(world)
        .flat_map(|g| g.0.iter().copied())
        .max()
        .map_or(1, |m| m + 1)
}

/// Rewrites cloned bodies' selection groups to fresh ids (one fresh id
/// per distinct source group), so duplicates form their *own* groups
/// instead of joining the originals' — the "group selection deteriorates
/// after repeated operations" bug.
pub(crate) fn remap_clone_groups(clones: &mut [BodyRecord], next_group: &mut u32) {
    let mut remap: Vec<(u32, u32)> = Vec::new();
    for record in clones {
        for group in &mut record.groups {
            let new = if let Some((_, new)) = remap.iter().find(|(old, _)| *old == *group) {
                *new
            } else {
                let new = *next_group;
                *next_group += 1;
                remap.push((*group, new));
                new
            };
            *group = new;
        }
    }
}

/// Clones the joints internal to a cloned body set: every joint whose
/// referenced bodies all lie within `id_map` is copied with fresh ids,
/// endpoints remapped, world anchors transformed by `map_world`, and
/// rest rotations advanced by `rot_offset` (the rotation the cloned
/// bodies received — radial arrays with rotated items).
pub(crate) fn clone_internal_joints(
    world: &mut World,
    id_map: &[(StableId, StableId)],
    map_world: impl Fn(Vec2) -> Vec2,
    rot_offset: f32,
) -> Vec<crate::command::snapshot::JointRecord> {
    let sources: Vec<StableId> = id_map.iter().map(|(old, _)| *old).collect();
    let remap = |id: StableId| {
        id_map
            .iter()
            .find(|(old, _)| *old == id)
            .map(|(_, new)| *new)
    };
    crate::command::joint_cmd::joints_referencing(world, &sources)
        .into_iter()
        .filter_map(|(_, record)| {
            let mut def = record.def;
            // Every endpoint must be inside the cloned set.
            def.body_a = remap(def.body_a)?;
            def.rest_rot_a += rot_offset;
            match def.body_b {
                Some(b) => {
                    def.body_b = Some(remap(b)?);
                    def.rest_rot_b += rot_offset;
                }
                // World pin: the anchor is a world point — transform it
                // like the cloned bodies. The pin itself never rotates,
                // so only `rest_rot_a` advanced.
                None => def.anchor_b = map_world(def.anchor_b),
            }
            Some(crate::command::snapshot::JointRecord {
                id: StableId::new(),
                def,
            })
        })
        .collect()
}

/// Duplicates a set of bodies at an offset, including the joints internal
/// to the set (a duplicated hinge assembly stays hinged).
///
/// Clone ids are generated once on first apply and reused on redo, so
/// later commands referencing the clones stay valid across undo/redo.
#[derive(Debug)]
pub struct DuplicateCommand {
    /// Bodies to clone.
    pub sources: Vec<StableId>,
    /// World-space offset applied to each clone.
    pub offset: Vec2,
    clones: Vec<BodyRecord>,
    joint_clones: Vec<crate::command::snapshot::JointRecord>,
}

impl DuplicateCommand {
    /// Builds a duplicate command for `sources` offset by `offset`.
    pub fn new(sources: Vec<StableId>, offset: Vec2) -> Self {
        Self {
            sources,
            offset,
            clones: Vec::new(),
            joint_clones: Vec::new(),
        }
    }
}

impl GameCommand for DuplicateCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.clones.is_empty() {
            // First application: capture sources and mint clone records.
            let mut clones = Vec::with_capacity(self.sources.len());
            let mut id_map = Vec::with_capacity(self.sources.len());
            for &id in &self.sources {
                let entity = resolve(world, id)?;
                let mut record =
                    BodyRecord::capture(world, entity).ok_or(CommandError::MissingEntity(id))?;
                record.id = StableId::new();
                record.pose.pos += self.offset;
                id_map.push((id, record.id));
                clones.push(record);
            }
            if clones.is_empty() {
                return Err(CommandError::NoEffect);
            }
            let offset = self.offset;
            self.joint_clones = clone_internal_joints(world, &id_map, |p| p + offset, 0.0);
            let mut next_group = next_group_id(world);
            remap_clone_groups(&mut clones, &mut next_group);
            self.clones = clones;
        }
        for record in &self.clones {
            record.spawn(world);
        }
        for record in &self.joint_clones {
            record.spawn(world);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for record in &self.joint_clones {
            let entity = resolve(world, record.id)?;
            world.despawn(entity);
        }
        for record in &self.clones {
            let entity = resolve(world, record.id)?;
            world.despawn(entity);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Duplicate"
    }
}
