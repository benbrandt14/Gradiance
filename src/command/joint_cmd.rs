//! Joint commands and the body↔joint cascade helper.

use crate::command::snapshot::JointRecord;
use crate::command::{CommandError, GameCommand};
use crate::core::ids::{IdIndex, StableId};
use crate::domain::joint::JointDef;
use bevy::prelude::*;

/// All joints referencing any of `body_ids` (for delete cascades and
/// duplicate/array cloning). World-pin joints reference one body.
pub fn joints_referencing(world: &mut World, body_ids: &[StableId]) -> Vec<(Entity, JointRecord)> {
    let mut query = world.query::<(Entity, &StableId, &JointDef)>();
    query
        .iter(world)
        .filter(|(_, _, def)| def.referenced_bodies().any(|id| body_ids.contains(&id)))
        .map(|(entity, id, def)| {
            (
                entity,
                JointRecord {
                    id: *id,
                    def: def.clone(),
                },
            )
        })
        .collect()
}

/// Spawns one joint from a record.
#[derive(Debug)]
pub struct SpawnJointCommand {
    /// The authored joint to create; its `id` is reused on redo.
    pub record: JointRecord,
}

impl GameCommand for SpawnJointCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        // Both endpoints must exist right now.
        for id in self.record.def.referenced_bodies() {
            world
                .resource::<IdIndex>()
                .entity(id)
                .ok_or(CommandError::MissingEntity(id))?;
        }
        self.record.spawn(world);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let entity = world
            .resource::<IdIndex>()
            .entity(self.record.id)
            .ok_or(CommandError::MissingEntity(self.record.id))?;
        world.despawn(entity);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Spawn joint"
    }
}

/// Deletes one joint, restoring it (same id) on undo.
#[derive(Debug)]
pub struct DeleteJointCommand {
    /// The joint to delete.
    pub id: StableId,
    /// Captured state for undo; filled during `apply`.
    record: Option<JointRecord>,
}

impl DeleteJointCommand {
    /// Builds a delete command for one joint.
    pub fn new(id: StableId) -> Self {
        Self { id, record: None }
    }
}

impl GameCommand for DeleteJointCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let entity = world
            .resource::<IdIndex>()
            .entity(self.id)
            .ok_or(CommandError::MissingEntity(self.id))?;
        self.record = JointRecord::capture(world, entity);
        if self.record.is_none() {
            return Err(CommandError::NoEffect);
        }
        world.despawn(entity);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        if let Some(record) = &self.record {
            record.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Delete joint"
    }
}
