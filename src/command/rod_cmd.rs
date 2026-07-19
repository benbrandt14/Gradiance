//! The rod composite command: one gesture, one undo step, several records.
//!
//! A rigid rod is a capsule body plus up to two end joints; a flexure rod
//! is an elastica joint plus up to two tip bodies. Either way the strut
//! tool must author them **atomically** — undo removes the whole rod, redo
//! restores it with the same ids (precedent:
//! [`DuplicateCommand`](crate::command::spawn::DuplicateCommand)).

use crate::command::snapshot::{BodyRecord, JointRecord};
use crate::command::{CommandError, GameCommand, resolve};
use bevy::prelude::*;

/// Everything one rod gesture authors: the bodies (a rigid rod's capsule,
/// or a flexure rod's tip circles) and the joints (end constraints, or
/// the elastica element).
#[derive(Debug, Clone, Reflect)]
pub struct RodSpec {
    /// Bodies to author (ids minted by the tool, stable across redo).
    pub bodies: Vec<BodyRecord>,
    /// Joints to author, referencing `bodies` and/or pre-existing bodies.
    pub joints: Vec<JointRecord>,
}

/// Spawns a rod's records atomically; undo despawns them all by id.
#[derive(Debug)]
pub struct SpawnRodCommand {
    /// The records to author.
    pub spec: RodSpec,
}

impl GameCommand for SpawnRodCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.spec.bodies.is_empty() && self.spec.joints.is_empty() {
            return Err(CommandError::NoEffect);
        }
        // Validate everything before touching the world so a failure
        // leaves it unchanged.
        for body in &self.spec.bodies {
            body.shape.validate()?;
        }
        for joint in &self.spec.joints {
            for id in joint.def.referenced_bodies() {
                let spawning = self.spec.bodies.iter().any(|b| b.id == id);
                if !spawning {
                    resolve(world, id)?;
                }
            }
        }
        for body in &self.spec.bodies {
            body.spawn(world);
        }
        for joint in &self.spec.joints {
            joint.spawn(world);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for joint in &self.spec.joints {
            let entity = resolve(world, joint.id)?;
            world.despawn(entity);
        }
        for body in &self.spec.bodies {
            let entity = resolve(world, body.id)?;
            world.despawn(entity);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::SPAWN_ROD
    }
}
