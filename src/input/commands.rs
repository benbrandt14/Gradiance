//! Command pattern implementation for Undo/Redo.
//!
//! Defines the `GameCommand` trait and the `CommandStack` resource.

use crate::prelude::*;
use crate::input::editable::EditableBox;
use crate::input::ZIndex;
use bevy_prototype_lyon::prelude::*;

/// A trait for game commands that support Undo/Redo.
pub trait GameCommand: Send + Sync {
    /// Apply the command to the world.
    fn apply(&mut self, world: &mut World);

    /// Revert the command's effects.
    fn undo(&mut self, world: &mut World);
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

        command.apply(world);
        self.history.push(command);
        self.index += 1;
    }

    /// Undoes the last command.
    pub fn undo(&mut self, world: &mut World) {
        if self.index > 0 {
            self.index -= 1;
            if let Some(command) = self.history.get_mut(self.index) {
                command.undo(world);
            }
        }
    }

    /// Redoes the previously undone command.
    pub fn redo(&mut self, world: &mut World) {
        if self.index < self.history.len() {
            if let Some(command) = self.history.get_mut(self.index) {
                command.apply(world);
            }
            self.index += 1;
        }
    }
}

/// Command to spawn a box.
pub struct SpawnBoxCommand {
    /// Position of the box.
    pub position: Vec2,
    /// Width of the box.
    pub width: f32,
    /// Height of the box.
    pub height: f32,
    /// The spawned entity ID (if active).
    pub entity: Option<Entity>,
}

impl SpawnBoxCommand {
    /// Create a new SpawnBoxCommand.
    pub fn new(position: Vec2, width: f32, height: f32) -> Self {
        Self {
            position,
            width,
            height,
            entity: None,
        }
    }
}

impl GameCommand for SpawnBoxCommand {
    fn apply(&mut self, world: &mut World) {
        // If entity exists and is valid, do nothing (idempotent check? or maybe we shouldn't fail)
        // Actually, on Redo, we need to spawn a NEW entity because the old one is likely dead.
        // Unless we used "Soft Delete".
        // Here we assume "Hard Delete" on Undo.

        // Get Z-Index
        let z = world.resource_mut::<ZIndex>().next();

        let shape = shapes::Rectangle {
            extents: Vec2::new(self.width, self.height),
            origin: shapes::RectangleOrigin::Center,
            ..default()
        };

        // We use world.spawn() but we need to construct the bundle.
        // Since we can't easily use the commands API inside a direct World mutation without a generic Commands queue,
        // we use World directly.

        // Note: ShapeBuilder returns a Bundle-like object, but we need to be careful with World::spawn.
        // World::spawn takes a Bundle.

        // Build the Lyon shape
        let geometry = ShapeBuilder::with(&shape)
            .fill(Color::srgb(0.5, 0.5, 1.0))
            .stroke(Stroke::new(Color::BLACK, 0.1))
            .build();

        let entity = world.spawn((
            geometry,
            RigidBody::Dynamic,
            Collider::rectangle(self.width as f64, self.height as f64),
            EditableBox {
                width: self.width as f64,
                height: self.height as f64,
            },
            Transform::from_xyz(self.position.x, self.position.y, z),
        )).id();

        self.entity = Some(entity);
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(entity) = self.entity {
            // Despawn
            if let Ok(entity_ref) = world.get_entity_mut(entity) {
                entity_ref.despawn();
            }
            self.entity = None;
        }
    }
}
