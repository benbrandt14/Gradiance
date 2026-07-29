//! Kinematic hold: engine-agnostic API for "park this body while a
//! gesture previews it".
//!
//! Tools put entities into [`KinematicHold`]; this seam swaps them to
//! kinematic (so the solver neither fights the preview nor launches the
//! body) and restores their engine body kind when released.
//!
//! The hold writes the *derived* engine role directly rather than the authored
//! `BodyPhysics`: a preview gesture is transient physical state, never an edit,
//! so it must not touch authored data or the undo stack (invariant #2). The
//! pre-hold role is remembered here for the same reason.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// Entities temporarily excluded from dynamics during a preview gesture.
#[derive(Resource, Default, Debug)]
pub struct KinematicHold {
    /// Currently held entities.
    pub entities: Vec<Entity>,
}

/// Applies/releases the hold as the resource changes.
pub fn apply_kinematic_hold(
    hold: Res<KinematicHold>,
    // The body kind each held entity had before the hold, to restore on release.
    mut held: Local<Vec<(Entity, RigidBody)>>,
    mut commands: Commands,
    bodies: Query<&RigidBody>,
    mut velocities: Query<&mut Velocity>,
) {
    if !hold.is_changed() {
        return;
    }
    // Release: entities we were holding that are no longer held.
    held.retain(|(entity, kind)| {
        if hold.entities.contains(entity) {
            true
        } else {
            commands.entity(*entity).try_insert(*kind);
            false
        }
    });
    // Acquire: newly held entities — remember their kind, then park them.
    for entity in hold.entities.iter().copied() {
        if !held.iter().any(|(e, _)| *e == entity) {
            let kind = bodies.get(entity).copied().unwrap_or(RigidBody::Dynamic);
            held.push((entity, kind));
            commands
                .entity(entity)
                .try_insert(RigidBody::KinematicPositionBased);
            if let Ok(mut velocity) = velocities.get_mut(entity) {
                *velocity = Velocity::zero();
            }
        }
    }
}
