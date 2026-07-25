//! Committing completed move/rotate gestures.

use crate::intent::TransformChange;
use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;

/// Applies the final poses of a completed gesture (and restores the
/// original poses on undo). One gesture — however many bodies — is one
/// command.
#[derive(Debug)]
pub struct CommitTransformCommand {
    /// Old and new pose per body.
    pub changes: Vec<TransformChange>,
}

impl CommitTransformCommand {
    fn set_poses(&self, world: &mut World, use_new: bool) -> Result<(), CommandError> {
        for change in &self.changes {
            let entity = resolve(world, change.id)?;
            let mut transform = world
                .get_mut::<Transform>(entity)
                .ok_or(CommandError::MissingEntity(change.id))?;
            let pose = if use_new { change.new } else { change.old };
            pose.apply_to(&mut transform);
        }
        Ok(())
    }
}

impl GameCommand for CommitTransformCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.changes.is_empty() {
            return Err(CommandError::NoEffect);
        }
        self.set_poses(world, true)
    }

    fn name(&self) -> &'static str {
        crate::intent::name::COMMIT_TRANSFORM
    }
}
