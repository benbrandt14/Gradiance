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

/// Deletes a set of bodies, restoring them (same ids) on undo.
#[derive(Debug)]
pub struct DeleteCommand {
    /// Bodies to delete.
    pub targets: Vec<StableId>,
    /// Captured state for undo; filled during `apply`.
    records: Vec<BodyRecord>,
}

impl DeleteCommand {
    /// Builds a delete command for `targets`.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self {
            targets,
            records: Vec::new(),
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
        self.records = pairs.iter().map(|(_, r)| r.clone()).collect();
        for (entity, _) in pairs {
            world.despawn(entity);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for record in &self.records {
            record.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Delete"
    }
}

/// Duplicates a set of bodies at an offset.
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
}

impl DuplicateCommand {
    /// Builds a duplicate command for `sources` offset by `offset`.
    pub fn new(sources: Vec<StableId>, offset: Vec2) -> Self {
        Self {
            sources,
            offset,
            clones: Vec::new(),
        }
    }
}

impl GameCommand for DuplicateCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.clones.is_empty() {
            // First application: capture sources and mint clone records.
            let mut clones = Vec::with_capacity(self.sources.len());
            for &id in &self.sources {
                let entity = resolve(world, id)?;
                let mut record =
                    BodyRecord::capture(world, entity).ok_or(CommandError::MissingEntity(id))?;
                record.id = StableId::new();
                record.pose.pos += self.offset;
                clones.push(record);
            }
            if clones.is_empty() {
                return Err(CommandError::NoEffect);
            }
            self.clones = clones;
        }
        for record in &self.clones {
            record.spawn(world);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
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
