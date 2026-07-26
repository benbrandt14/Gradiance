//! Whole-scene replacement as an undoable command.

use crate::{CommandError, GameCommand};
use bevy::prelude::*;
use gradiance_scene::SceneRecord;

/// Replaces the entire authored world with a scene.
///
/// **Loading a scene is undoable** — an accidental load never destroys work.
/// The stack snapshots the pre-load world, so the command carries nothing but
/// the scene to install.
#[derive(Debug)]
pub struct LoadSceneCommand {
    /// The scene to load.
    pub incoming: SceneRecord,
}

impl LoadSceneCommand {
    /// Builds a load command.
    pub fn new(incoming: SceneRecord) -> Self {
        Self { incoming }
    }
}

impl GameCommand for LoadSceneCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        for body in &self.incoming.bodies {
            body.shape.validate()?;
        }
        self.incoming.apply(world);
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::LOAD_SCENE
    }
}
