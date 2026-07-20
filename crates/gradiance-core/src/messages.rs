//! Message-queue helpers shared by the exclusive-system seams.

use bevy::prelude::*;

/// Drains every pending message of type `M` from the world.
///
/// The standard first step of an exclusive system that owns a message
/// queue (the command dispatcher, the persistence handler): take the whole
/// batch, then process it against `&mut World` without holding a borrow.
pub fn drain<M: Message>(world: &mut World) -> Vec<M> {
    world
        .get_resource_mut::<Messages<M>>()
        .map(|mut m| m.drain().collect())
        .unwrap_or_default()
}
