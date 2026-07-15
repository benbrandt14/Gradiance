//! Property edits: typed authored-component changes, batched and undoable.

use crate::command::{CommandError, GameCommand, resolve};
use crate::core::ids::StableId;
use crate::domain::appearance::Appearance;
use crate::domain::depth::DepthBand;
use crate::domain::joint::JointDef;
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
    /// Field source (`None` = no field).
    Field(Option<crate::domain::field::FieldSource>),
    /// Trajectory-trail marker (`None` = no tracer).
    Tracer(Option<crate::domain::tracer::Tracer>),
    /// Rotation-lock flag (`LockedAxes::ROTATION_LOCKED` presence).
    RotationLock(bool),
    /// Visual appearance.
    Appearance(Appearance),
    /// Authored depth band (collision volume ≡ render depth).
    Depth(DepthBand),
    /// Full joint definition (limits, motors, collide flags, anchors).
    Joint(JointDef),
    /// A behavior node's kind (sensor quantity/signal, actuator wiring,
    /// tracer fade). Editing wires the dataflow — undoable like any prop.
    NodeKind(crate::domain::node::NodeKind),
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
            Self::Field(field) => match field {
                Some(f) => {
                    entity_mut.insert(*f);
                }
                None => {
                    entity_mut.remove::<crate::domain::field::FieldSource>();
                }
            },
            Self::Tracer(tracer) => match tracer {
                Some(t) => {
                    entity_mut.insert(*t);
                }
                None => {
                    entity_mut.remove::<crate::domain::tracer::Tracer>();
                }
            },
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
            Self::Depth(v) => {
                entity_mut.insert(v.sanitized());
            }
            Self::Joint(v) => {
                entity_mut.insert(v.clone());
            }
            Self::NodeKind(kind) => {
                entity_mut.insert(kind.clone());
                // A tracer node also carries the `Tracer` component the
                // trail sampler reads; keep it in sync (and off other kinds).
                match kind {
                    crate::domain::node::NodeKind::Tracer(tracer) => {
                        entity_mut.insert(*tracer);
                    }
                    _ => {
                        entity_mut.remove::<crate::domain::tracer::Tracer>();
                    }
                }
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
