//! Property edits: typed authored-component changes, batched and undoable.

use crate::{CommandError, GameCommand, resolve};
use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::joint::JointDef;
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
    Field(Option<gradiance_domain::field::FieldSource>),
    /// Fluid region (`None` = not a fluid; brief N-7).
    Fluid(Option<gradiance_domain::fluid::Fluid>),
    /// Refractive index (`None` = a mirror to beams; brief S-3).
    Optics(Option<gradiance_domain::optics::Optics>),
    /// Initial velocity and spin (`None` = starts at rest; brief S-1).
    InitialMotion(Option<gradiance_domain::motion::InitialMotion>),
    /// Trajectory-trail marker (`None` = no tracer).
    Tracer(Option<gradiance_domain::tracer::Tracer>),
    /// Rotation-lock flag (`LockedAxes::ROTATION_LOCKED` presence).
    RotationLock(bool),
    /// Visual appearance.
    Appearance(Appearance),
    /// Authored depth band (collision volume ≡ render depth).
    Depth(DepthBand),
    /// Full joint definition (limits, motors, collide flags, anchors).
    Joint(JointDef),
    /// A behavior node's kind (tracer fade, thruster thrust and axis).
    /// Undoable like any prop.
    NodeKind(gradiance_domain::node::NodeKind),
    /// A behavior node's placement: the attachment (which body it rides,
    /// at what offset, or free) and its own pose (where a free node sits).
    /// The pose half is what a free node saves; an attached node's
    /// transform is re-derived by the follow system either way.
    NodePlace(
        gradiance_domain::node::NodeAttachment,
        gradiance_core::units::PosRot,
    ),
}

/// Inserts `value` on the entity, or removes the component when `None`: an
/// optional authored component (field, fluid, optics, tracer).
fn set_optional<C: Component>(entity: &mut EntityWorldMut, value: Option<C>) {
    match value {
        Some(c) => {
            entity.insert(c);
        }
        None => {
            entity.remove::<C>();
        }
    }
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
            Self::Field(field) => set_optional(&mut entity_mut, *field),
            Self::Fluid(fluid) => set_optional(
                &mut entity_mut,
                fluid.map(gradiance_domain::fluid::Fluid::sanitized),
            ),
            Self::Optics(optics) => set_optional(
                &mut entity_mut,
                optics.map(gradiance_domain::optics::Optics::sanitized),
            ),
            // A motion at rest is no motion: the record stays empty.
            Self::InitialMotion(motion) => {
                match motion.map(gradiance_domain::motion::InitialMotion::sanitized) {
                    Some(m) if !m.is_rest() => {
                        entity_mut.insert(m);
                    }
                    _ => {
                        entity_mut.remove::<gradiance_domain::motion::InitialMotion>();
                    }
                }
            }
            Self::Tracer(tracer) => set_optional(&mut entity_mut, *tracer),
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
                // A tracer node also carries the `Tracer` component the trail
                // sampler reads; keep it in sync (and drop it from a node that
                // is no longer a tracer).
                match kind.tracer() {
                    Some(tracer) => {
                        entity_mut.insert(tracer);
                    }
                    None => {
                        entity_mut.remove::<gradiance_domain::tracer::Tracer>();
                    }
                }
            }
            Self::NodePlace(attachment, pose) => {
                entity_mut.insert(*attachment);
                if let Some(mut t) = entity_mut.get_mut::<Transform>() {
                    pose.apply_to(&mut t);
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

    fn name(&self) -> &'static str {
        crate::intent::name::PROPERTY_EDIT
    }
}

/// The changes that set one body's fluid region to `new` (brief N-7), given
/// its current fluid, sensor flag and role. Turning a body into a fluid
/// also makes it a sensor (bodies pass into it) and, if it was dynamic,
/// static (a dynamic sensor would fall through the floor); all of it is one
/// undo step. Turning the fluid off keeps the sensor flag and the role. An
/// unchanged value adds no change. Shared by the inspector's fluid toggle
/// and `(set-fluid …)`, so the two cannot disagree.
#[must_use]
pub fn fluid_changes(
    id: StableId,
    current: Option<gradiance_domain::fluid::Fluid>,
    sensor: bool,
    rigid_body: RigidBody,
    new: Option<gradiance_domain::fluid::Fluid>,
) -> Vec<PropertyChange> {
    let new = new.map(gradiance_domain::fluid::Fluid::sanitized);
    let mut changes = Vec::new();
    if current != new {
        changes.push(PropertyChange {
            id,
            old: PropertyValue::Fluid(current),
            new: PropertyValue::Fluid(new),
        });
    }
    if current.is_none() && new.is_some() {
        if !sensor {
            changes.push(PropertyChange {
                id,
                old: PropertyValue::Sensor(false),
                new: PropertyValue::Sensor(true),
            });
        }
        if rigid_body == RigidBody::Dynamic {
            changes.push(PropertyChange {
                id,
                old: PropertyValue::RigidBody(rigid_body),
                new: PropertyValue::RigidBody(RigidBody::Static),
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_domain::fluid::Fluid;

    #[test]
    fn making_a_fluid_makes_a_static_sensor_in_one_batch() {
        let id = StableId::new();
        let changes = fluid_changes(id, None, false, RigidBody::Dynamic, Some(Fluid::default()));
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].new, PropertyValue::Fluid(Some(Fluid::default())));
        assert_eq!(changes[1].new, PropertyValue::Sensor(true));
        assert_eq!(changes[2].new, PropertyValue::RigidBody(RigidBody::Static));
        // A kinematic sensor keeps its role and flag.
        let changes = fluid_changes(id, None, true, RigidBody::Kinematic, Some(Fluid::default()));
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn an_edit_or_removal_touches_only_the_fluid() {
        let id = StableId::new();
        let old = Some(Fluid::default());
        let thicker = Some(Fluid {
            drag: 5.0,
            ..Fluid::default()
        });
        assert_eq!(
            fluid_changes(id, old, true, RigidBody::Static, thicker).len(),
            1
        );
        let off = fluid_changes(id, old, true, RigidBody::Static, None);
        assert_eq!(off.len(), 1);
        assert_eq!(off[0].new, PropertyValue::Fluid(None));
        assert!(fluid_changes(id, old, true, RigidBody::Static, old).is_empty());
    }
}
