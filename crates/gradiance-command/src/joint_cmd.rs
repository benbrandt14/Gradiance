//! Joint commands and the body↔joint cascade helper.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::joint::JointDef;
use gradiance_scene::JointRecord;

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
            resolve(world, id)?;
        }
        self.record.spawn(world);
        // A prismatic pinned to the background rotation-locks its body by
        // default (authored — unlock any time in the inspector): the joint
        // is a *guide*, and a guided body has no business spinning.
        if self.record.def.body_b.is_none()
            && matches!(
                self.record.def.kind,
                gradiance_domain::joint::JointKind::Slider { .. }
            )
            && let Ok(body) = resolve(world, self.record.def.body_a)
            && let Ok(mut body_mut) = world.get_entity_mut(body)
        {
            // Authored intent, not a derived engine component: the physics
            // layer composes the engine's locked-axis set from this flag and
            // the body's simulation-plane constraint, in one place.
            if let Some(mut physics) = body_mut.get_mut::<gradiance_domain::props::BodyPhysics>() {
                physics.rotation_locked = true;
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::SPAWN_JOINT
    }
}

/// Deletes one joint, restoring it (same id) on undo.
#[derive(Debug)]
pub struct DeleteJointCommand {
    /// The joint to delete.
    pub id: StableId,
}

impl DeleteJointCommand {
    /// Builds a delete command for one joint.
    pub fn new(id: StableId) -> Self {
        Self { id }
    }
}

impl GameCommand for DeleteJointCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let entity = resolve(world, self.id)?;
        // Capture only to confirm this really is a joint; the stack owns the
        // state needed to bring it back.
        if JointRecord::capture(world, entity).is_none() {
            return Err(CommandError::NoEffect);
        }
        world.despawn(entity);
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::DELETE_JOINT
    }
}
