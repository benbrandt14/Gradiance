//! Property edits: typed authored-component changes, batched and undoable.

use crate::command::{CommandError, GameCommand};
use crate::core::ids::{IdIndex, StableId};
use crate::domain::appearance::Appearance;
use crate::domain::joint::JointDef;
use crate::domain::layers::LayerMask32;
use crate::domain::props::PhysicalProps;
use crate::domain::shape::ShapeDef;
use bevy::prelude::*;

/// A snapshot of one editable authored component.
///
/// Adding a new editable component = one variant + one arm in
/// [`PropertyValue::write`]/[`read`](PropertyValue::read).
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// Body geometry.
    Shape(ShapeDef),
    /// Physical properties.
    Props(PhysicalProps),
    /// Visual appearance.
    Appearance(Appearance),
    /// Collision layers / depth.
    Layers(LayerMask32),
    /// Full joint definition (limits, motors, collide flags, anchors).
    Joint(JointDef),
}

impl PropertyValue {
    fn write(&self, world: &mut World, entity: Entity) -> Result<(), CommandError> {
        let mut entity_mut = world
            .get_entity_mut(entity)
            .map_err(|_| CommandError::NoEffect)?;
        match self {
            Self::Shape(v) => {
                v.validate()?;
                entity_mut.insert(v.clone());
            }
            Self::Props(v) => {
                entity_mut.insert(*v);
            }
            Self::Appearance(v) => {
                entity_mut.insert(*v);
            }
            Self::Layers(v) => {
                entity_mut.insert(*v);
            }
            Self::Joint(v) => {
                entity_mut.insert(v.clone());
            }
        }
        Ok(())
    }
}

/// One target's old → new property change.
#[derive(Debug, Clone)]
pub struct PropertyChange {
    /// The authored entity being edited.
    pub id: StableId,
    /// Value before the edit (restored on undo).
    pub old: PropertyValue,
    /// Value after the edit.
    pub new: PropertyValue,
}

/// Applies a batch of property changes as one undo step (multi-select
/// edits are one gesture, one command).
#[derive(Debug)]
pub struct SetPropertyCommand {
    /// The changes to apply.
    pub changes: Vec<PropertyChange>,
}

impl SetPropertyCommand {
    fn write_all(&self, world: &mut World, use_new: bool) -> Result<(), CommandError> {
        for change in &self.changes {
            let entity = world
                .resource::<IdIndex>()
                .entity(change.id)
                .ok_or(CommandError::MissingEntity(change.id))?;
            let value = if use_new { &change.new } else { &change.old };
            value.write(world, entity)?;
        }
        Ok(())
    }
}

impl GameCommand for SetPropertyCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.changes.is_empty() {
            return Err(CommandError::NoEffect);
        }
        self.write_all(world, true)
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        self.write_all(world, false)
    }

    fn name(&self) -> &'static str {
        "Edit properties"
    }
}
