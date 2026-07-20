//! Joint commands and the body↔joint cascade helper.

use crate::scene::JointRecord;
use crate::command::{CommandError, GameCommand, resolve};
use crate::core::ids::StableId;
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
    /// Whether body A was rotation-locked before this command (world-pin
    /// prismatics lock the pinned body by default; undo restores).
    pub locked_before: Option<bool>,
}

impl GameCommand for SpawnJointCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        // Both endpoints must exist right now.
        for id in self.record.def.referenced_bodies() {
            resolve(world, id)?;
        }
        self.record.spawn(world);
        // A prismatic pinned to the background rotation-locks its body by
        // default (authored — unlock any time in the inspector): the joint
        // is a *guide*, and a guided body has no business spinning.
        if self.record.def.body_b.is_none()
            && matches!(
                self.record.def.kind,
                crate::domain::joint::JointKind::Slider { .. }
            )
            && let Ok(body) = resolve(world, self.record.def.body_a)
            && let Ok(mut body_mut) = world.get_entity_mut(body)
        {
            let was_locked = body_mut.contains::<avian2d::prelude::LockedAxes>();
            self.locked_before = Some(was_locked);
            if !was_locked {
                body_mut.insert(avian2d::prelude::LockedAxes::ROTATION_LOCKED);
            }
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let entity = resolve(world, self.record.id)?;
        world.despawn(entity);
        if self.locked_before == Some(false)
            && let Ok(body) = resolve(world, self.record.def.body_a)
            && let Ok(mut body_mut) = world.get_entity_mut(body)
        {
            body_mut.remove::<avian2d::prelude::LockedAxes>();
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::SPAWN_JOINT
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
        let entity = resolve(world, self.id)?;
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
        crate::command::intent::name::DELETE_JOINT
    }
}
