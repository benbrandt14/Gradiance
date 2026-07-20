//! Kinematic hold: engine-agnostic API for "park this body while a
//! gesture previews it".
//!
//! Tools put entities into [`KinematicHold`]; this seam swaps them to
//! kinematic (so the solver neither fights the preview nor launches the
//! body) and restores their authored body kind when released. Post-collapse
//! `RigidBody` is both the authored and the live kind, so the pre-hold kind
//! is remembered here rather than read from a separate mirror.

use avian2d::prelude::*;
use bevy::prelude::*;

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
    mut velocities: Query<(&mut LinearVelocity, &mut AngularVelocity)>,
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
            commands.entity(entity).try_insert(RigidBody::Kinematic);
            if let Ok((mut lin, mut ang)) = velocities.get_mut(entity) {
                lin.0 = Vec2::ZERO;
                ang.0 = 0.0;
            }
        }
    }
}
