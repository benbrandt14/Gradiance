//! Property edits: typed authored-component changes, batched and undoable.

use crate::command::{CommandError, GameCommand, resolve};
use crate::core::ids::StableId;
use crate::domain::appearance::Appearance;
use crate::domain::joint::JointDef;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use avian2d::prelude::*;
use bevy::prelude::*;

/// A snapshot of one editable authored component.
///
/// Adding a new editable component = one variant + one arm in this
/// enum's `write`/`read` methods.
#[derive(Debug, Clone, PartialEq, Reflect)]
pub enum PropertyValue {
    /// Body geometry.
    Shape(ShapeDef),
    /// Simulation role (dynamic/static/kinematic).
    RigidBody(RigidBody),
    /// Coulomb friction.
    Friction(Friction),
    /// Bounciness.
    Restitution(Restitution),
    /// Mass density.
    Density(ColliderDensity),
    /// Per-body gravity multiplier.
    GravityScale(GravityScale),
    /// Overlap-sensor flag (marker presence).
    Sensor(bool),
    /// Rotation-lock flag (`LockedAxes::ROTATION_LOCKED` presence).
    RotationLock(bool),
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
            Self::RigidBody(v) => {
                entity_mut.insert(*v);
            }
            Self::Friction(v) => {
                entity_mut.insert(*v);
            }
            Self::Restitution(v) => {
                entity_mut.insert(*v);
            }
            Self::Density(v) => {
                entity_mut.insert(*v);
            }
            Self::GravityScale(v) => {
                entity_mut.insert(*v);
            }
            Self::Sensor(on) => {
                if *on {
                    entity_mut.insert(Sensor);
                } else {
                    entity_mut.remove::<Sensor>();
                }
            }
            Self::RotationLock(on) => {
                if *on {
                    entity_mut.insert(LockedAxes::ROTATION_LOCKED);
                } else {
                    entity_mut.remove::<LockedAxes>();
                }
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
#[derive(Debug, Clone, Reflect)]
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
pub struct PropertyEditCommand {
    /// The changes to apply.
    pub changes: Vec<PropertyChange>,
}

impl PropertyEditCommand {
    fn write_all(&self, world: &mut World, use_new: bool) -> Result<(), CommandError> {
        for change in &self.changes {
            let entity = resolve(world, change.id)?;
            let value = if use_new { &change.new } else { &change.old };
            value.write(world, entity)?;
        }
        Ok(())
    }
}

impl GameCommand for PropertyEditCommand {
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
        crate::command::intent::name::PROPERTY_EDIT
    }
}
