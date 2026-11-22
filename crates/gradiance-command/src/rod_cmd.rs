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
//! existing bodies), hence this one. A chain (brief N-5) is the same thing
//! with many bodies: its links and hinges go through this command too.
//!
//! [`ShapeDef::Capsule`]: gradiance_domain::shape::ShapeDef::Capsule

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_scene::{BodyRecord, JointRecord};

/// Spawns a rod's body (or a chain's links) and their joints atomically.
#[derive(Debug)]
pub struct SpawnRodCommand {
    /// The new bodies: one capsule for a rod, the links for a chain (ids
    /// minted by the tool or the operation, stable across redo).
    pub bodies: Vec<BodyRecord>,
    /// Joints, each referencing new bodies and/or existing ones (or the
    /// world).
    pub joints: Vec<JointRecord>,
    /// The undo label: [`SPAWN_ROD`](crate::intent::name::SPAWN_ROD) or
    /// [`SPAWN_CHAIN`](crate::intent::name::SPAWN_CHAIN).
    name: &'static str,
}

impl SpawnRodCommand {
    /// Builds the command from the rod tool's records.
    pub fn new(body: BodyRecord, joints: Vec<JointRecord>) -> Self {
        Self {
            bodies: vec![body],
            joints,
            name: crate::intent::name::SPAWN_ROD,
        }
    }

    /// A chain (brief N-5): its links and the hinges between them and at
    /// its pinned ends, one undo step, the same seam as the rod.
    pub fn chain(bodies: Vec<BodyRecord>, joints: Vec<JointRecord>) -> Self {
        Self {
            bodies,
            joints,
            name: crate::intent::name::SPAWN_CHAIN,
        }
    }
}

impl GameCommand for SpawnRodCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        // Validate everything before touching the world, so a failure
        // leaves it unchanged.
        for body in &self.bodies {
            body.shape.validate()?;
        }
        for joint in &self.joints {
            for id in joint.def.referenced_bodies() {
                if !self.bodies.iter().any(|b| b.id == id) {
                    resolve(world, id)?;
                }
            }
        }
        for body in &self.bodies {
            body.spawn(world);
        }
        for joint in &self.joints {
            joint.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// Turns a body into a sponge (brief S-12): deletes `target`, then spawns
/// the lattice circles in one fresh group and the springs between them.
/// One undo step.
#[derive(Debug)]
pub struct SpongeCommand {
    /// The body the sponge replaces.
    pub target: StableId,
    /// The lattice circles.
    pub bodies: Vec<BodyRecord>,
    /// The springs between lattice neighbours.
    pub joints: Vec<JointRecord>,
}

impl GameCommand for SpongeCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        for body in &self.bodies {
            body.shape.validate()?;
        }
        crate::spawn::DeleteCommand::new(vec![self.target]).apply(world)?;
        // One group, minted against the world after the delete, so a redo
        // mints the same id.
        let group = crate::group_cmd::fresh_group_id(world);
        let mut spawn = SpawnRodCommand {
            bodies: self.bodies.clone(),
            joints: self.joints.clone(),
            name: crate::intent::name::SPONGE,
        };
        for body in &mut spawn.bodies {
            body.groups = vec![group];
        }
        spawn.apply(world)
    }

    fn name(&self) -> &'static str {
        crate::intent::name::SPONGE
    }
}
