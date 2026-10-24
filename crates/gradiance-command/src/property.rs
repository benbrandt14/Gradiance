//! Property edits: typed authored-component changes, batched and undoable.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::joint::JointDef;
use gradiance_domain::props::{BodyKind, BodyPhysics, Density};
use gradiance_domain::shape::ShapeDef;

/// A snapshot of one editable authored component.
///
/// Adding a new editable component = one variant + one arm in this
/// enum's `write`/`read` methods.
#[derive(Debug, Clone, PartialEq, Reflect)]
pub enum PropertyValue {
    /// Body geometry.
    Shape(ShapeDef),
    /// Simulation role (dynamic/static/kinematic).
    BodyKind(BodyKind),
    /// Coulomb friction coefficient.
    Friction(f32),
    /// Bounciness.
    Restitution(f32),
    /// Areal mass density.
    Density(Density),
    /// Per-body gravity multiplier.
    GravityScale(f32),
    /// Overlap-sensor flag.
    Sensor(bool),
    /// Field source (`None` = no field).
    Field(Option<gradiance_domain::field::FieldSource>),
    /// Trajectory-trail marker (`None` = no tracer).
    Tracer(Option<gradiance_domain::tracer::Tracer>),
    /// Rotation-lock flag (authored intent; the engine's locked-axis set is
    /// derived from it in the physics layer).
    RotationLock(bool),
    /// Visual appearance.
    Appearance(Appearance),
    /// Authored depth band (collision volume ≡ render depth).
    Depth(DepthBand),
    /// Full joint definition (limits, motors, collide flags, anchors).
    Joint(JointDef),
    /// A behavior node's kind (sensor quantity/signal, actuator wiring,
    /// tracer fade). Editing wires the dataflow — undoable like any prop.
    NodeKind(gradiance_domain::node::NodeKind),
}

/// The authored physics of a body being edited.
///
/// A body without one is not an authored body, so an edit aimed at it is a
/// no-op rather than an error worth surfacing.
fn physics_mut<'a>(
    entity_mut: &'a mut EntityWorldMut,
) -> Result<Mut<'a, BodyPhysics>, CommandError> {
    entity_mut
        .get_mut::<BodyPhysics>()
        .ok_or(CommandError::NoEffect)
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
            // Every physics property edits one field of the authored
            // `BodyPhysics`; the physics layer turns that into engine
            // components on `Changed<>`. Nothing here names the engine.
            Self::BodyKind(v) => {
                physics_mut(&mut entity_mut)?.kind = *v;
            }
            Self::Friction(v) => {
                physics_mut(&mut entity_mut)?.friction = *v;
            }
            Self::Restitution(v) => {
                physics_mut(&mut entity_mut)?.restitution = *v;
            }
            Self::Density(v) => {
                physics_mut(&mut entity_mut)?.density = *v;
            }
            Self::GravityScale(v) => {
                physics_mut(&mut entity_mut)?.gravity_scale = *v;
            }
            Self::Field(field) => match field {
                Some(f) => {
                    entity_mut.insert(*f);
                }
                None => {
                    entity_mut.remove::<gradiance_domain::field::FieldSource>();
                }
            },
            Self::Tracer(tracer) => match tracer {
                Some(t) => {
                    entity_mut.insert(*t);
                }
                None => {
                    entity_mut.remove::<gradiance_domain::tracer::Tracer>();
                }
            },
            Self::Sensor(on) => {
                physics_mut(&mut entity_mut)?.sensor = *on;
            }
            Self::RotationLock(on) => {
                physics_mut(&mut entity_mut)?.rotation_locked = *on;
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
                // A tracer node also carries the `Tracer` component the trail
                // sampler reads; keep it in sync.
                let gradiance_domain::node::NodeKind::Tracer(tracer) = kind;
                entity_mut.insert(*tracer);
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

    fn name(&self) -> &'static str {
        crate::intent::name::PROPERTY_EDIT
    }
}
