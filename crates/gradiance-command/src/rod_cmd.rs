//! The rod command: one gesture, one undo step, several records.
//!
//! A rigid rod is a capsule body ([`ShapeDef::Capsule`]) plus a hinge for
//! each end that landed on another body. The rod tool authors them
//! **atomically** — one command, so one undo removes the whole rod and one
//! redo restores it with the same ids. (Undo is snapshot-based —
//! [`CommandStack`](crate::CommandStack) — so the command only applies.)
//!
//! No existing command spawned a body *and* joints referencing it from
//! fresh records ([`SpawnCommand`](crate::spawn::SpawnCommand) is one
//! record; [`DuplicateCommand`](crate::spawn::DuplicateCommand) clones
//! existing bodies), hence this one.
//!
//! [`ShapeDef::Capsule`]: gradiance_domain::shape::ShapeDef::Capsule

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_scene::{BodyRecord, JointRecord};

/// Spawns a rod's body and end joints atomically.
#[derive(Debug)]
pub struct SpawnRodCommand {
    /// The rod body (a capsule; id minted by the tool, stable across redo).
    pub body: BodyRecord,
    /// End joints, each referencing [`body`](Self::body) and/or an existing
    /// body.
    pub joints: Vec<JointRecord>,
}

impl SpawnRodCommand {
    /// Builds the command from the tool's records.
    pub fn new(body: BodyRecord, joints: Vec<JointRecord>) -> Self {
        Self { body, joints }
    }
}

impl GameCommand for SpawnRodCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        // Validate everything before touching the world, so a failure
        // leaves it unchanged.
        self.body.shape.validate()?;
        for joint in &self.joints {
            for id in joint.def.referenced_bodies() {
                if id != self.body.id {
                    resolve(world, id)?;
                }
            }
        }
        self.body.spawn(world);
        for joint in &self.joints {
            joint.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::SPAWN_ROD
    }
}
