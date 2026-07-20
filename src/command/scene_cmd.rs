//! Whole-scene replacement as an undoable command.

use crate::scene::SceneRecord;
use crate::command::{CommandError, GameCommand};
use bevy::prelude::*;

/// Replaces the entire authored world with a scene.
///
/// The previous world is captured on first apply, so **loading a scene is
/// undoable** — an accidental load never destroys work.
#[derive(Debug)]
pub struct LoadSceneCommand {
    /// The scene to load.
    pub incoming: SceneRecord,
    previous: Option<SceneRecord>,
}

impl LoadSceneCommand {
    /// Builds a load command.
    pub fn new(incoming: SceneRecord) -> Self {
        Self {
            incoming,
            previous: None,
        }
    }
}

impl GameCommand for LoadSceneCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        for body in &self.incoming.bodies {
            body.shape.validate()?;
        }
        if self.previous.is_none() {
            self.previous = Some(SceneRecord::capture(world));
        }
        self.incoming.apply(world);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        if let Some(previous) = &self.previous {
            previous.apply(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::LOAD_SCENE
    }
}
