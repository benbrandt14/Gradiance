//! Kinematic hold: engine-agnostic API for "park this body while a
//! gesture previews it".
//!
//! Tools put entities into [`KinematicHold`]; this seam swaps them to
//! kinematic (so the solver neither fights the preview nor launches the
//! body) and restores their authored body kind when released.

use crate::domain::props::{BodyKind, PhysicalProps};
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
    mut previous: Local<Vec<Entity>>,
    mut commands: Commands,
    props: Query<&PhysicalProps>,
    mut velocities: Query<(&mut LinearVelocity, &mut AngularVelocity)>,
) {
    if !hold.is_changed() {
        return;
    }
    for entity in previous.iter().copied() {
        if !hold.entities.contains(&entity) {
            // Restore the authored body kind.
            if let Ok(p) = props.get(entity) {
                let kind = match p.body {
                    BodyKind::Dynamic => RigidBody::Dynamic,
                    BodyKind::Static => RigidBody::Static,
                    BodyKind::Kinematic => RigidBody::Kinematic,
                };
                commands.entity(entity).try_insert(kind);
            }
        }
    }
    for entity in hold.entities.iter().copied() {
        if !previous.contains(&entity) {
            commands.entity(entity).try_insert(RigidBody::Kinematic);
            if let Ok((mut lin, mut ang)) = velocities.get_mut(entity) {
                lin.0 = Vec2::ZERO;
                ang.0 = 0.0;
            }
        }
    }
    previous.clone_from(&hold.entities);
}
