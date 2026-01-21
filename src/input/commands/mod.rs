//! Command pattern implementation for Undo/Redo.
//!
//! Defines the `GameCommand` trait and the `CommandStack` resource.

pub mod shapes;
pub mod joints;

pub use shapes::*;
pub use joints::*;

use crate::prelude::*;

/// A trait for game commands that support Undo/Redo.
pub trait GameCommand: Send + Sync {
    /// Apply the command to the world.
    fn apply(&mut self, world: &mut World) -> Result<(), String>;

    /// Revert the command's effects.
    fn undo(&mut self, world: &mut World);

    /// Returns the name of the command.
    fn name(&self) -> String;
}

/// Resource handling the stack of executed commands.
#[derive(Resource, Default)]
pub struct CommandStack {
    /// The stack of commands.
    history: Vec<Box<dyn GameCommand>>,
    /// The current position in the stack (points to the next slot to write).
    /// If index < history.len(), we are in a "Redo" state.
    index: usize,
}

impl CommandStack {
    /// Pushes a new command and executes it.
    /// Clears any redo history.
    pub fn push(&mut self, mut command: Box<dyn GameCommand>, world: &mut World) {
        // If we are in the middle of the stack (undo performed), clear the future.
        if self.index < self.history.len() {
            self.history.truncate(self.index);
        }

        match command.apply(world) {
            Ok(_) => {
                info!("Command Applied: {}", command.name());
                self.history.push(command);
                self.index += 1;
            }
            Err(e) => {
                warn!("Command Failed: {}: {}", command.name(), e);
            }
        }
    }

    /// Undoes the last command.
    pub fn undo(&mut self, world: &mut World) {
        if self.index > 0 {
            self.index -= 1;
            if let Some(command) = self.history.get_mut(self.index) {
                info!("Undo: {}", command.name());
                command.undo(world);
            }
        }
    }

    /// Redoes the previously undone command.
    pub fn redo(&mut self, world: &mut World) {
        if self.index < self.history.len() {
            if let Some(command) = self.history.get_mut(self.index) {
                if let Err(e) = command.apply(world) {
                    warn!("Redo Failed: {}: {}", command.name(), e);
                } else {
                    info!("Redo: {}", command.name());
                }
            }
            self.index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};
    use crate::input::ZIndex as GameZIndex;
    use crate::input::editable::EditableCircle;

    #[fixture]
    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<GameZIndex>();
        world
    }

    #[rstest]
    fn test_command_stack(mut world: World) {
        let mut stack = CommandStack::default();

        // 1. Push Box
        let box_cmd = Box::new(SpawnBoxCommand::new(Vec2::ZERO, 1.0, 1.0));
        stack.push(box_cmd, &mut world);

        assert_eq!(stack.index, 1);
        assert_eq!(stack.history.len(), 1);
        assert_eq!(world.entities().len(), 1);

        // 2. Undo
        stack.undo(&mut world);
        assert_eq!(stack.index, 0);
        assert_eq!(stack.history.len(), 1);
        assert_eq!(world.entities().len(), 0);

        // 3. Redo
        stack.redo(&mut world);
        assert_eq!(stack.index, 1);
        assert_eq!(world.entities().len(), 1);

        // 4. Undo again
        stack.undo(&mut world);
        assert_eq!(stack.index, 0);
        assert_eq!(world.entities().len(), 0);

        // 5. Push new command (Circle), should truncate history
        let circle_cmd = Box::new(SpawnCircleCommand {
            position: Vec2::new(10.0, 0.0),
            radius: 1.0,
            entity: None,
        });
        stack.push(circle_cmd, &mut world);

        assert_eq!(stack.index, 1);
        assert_eq!(stack.history.len(), 1); // Previous box command should be removed
        assert_eq!(world.entities().len(), 1);

        // Verify it is indeed the circle (by checking component)
        let entity = world.iter_entities().next().unwrap().id();
        assert!(world.get::<EditableCircle>(entity).is_some());
    }
}
