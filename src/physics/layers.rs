//! Collision layers and exclusion logic.
//!
//! This module defines components and resources for managing physics layers
//! and preventing collisions between specific groups of entities (e.g. selection groups).

use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier::pipeline::{PhysicsHooks, PairFilterContext, ContactModificationContext};
use bevy_rapier2d::prelude::SolverFlags;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Component defining which physics layers an entity belongs to.
/// Each bit represents a layer (Bit 0 = A, Bit 1 = B, etc.).
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct PhysicsLayers(pub u32);

impl Default for PhysicsLayers {
    fn default() -> Self {
        Self(1) // Layer A
    }
}

/// Component to disable collisions between entities in the same "exclusion group".
/// Entities with the same group ID will not collide.
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct DisableSelfCollision(pub u32);

/// Resource for managing collision exclusions.
/// Shared between ECS and Physics Thread via Arc/RwLock.
#[derive(Resource, Clone, Default)]
pub struct CollisionExclusionMap {
    /// Shared map mapping Entity to Exclusion Group ID.
    pub map: Arc<RwLock<HashMap<Entity, u32>>>,
}

/// Custom Physics Hook to filter collisions.
pub struct ExclusionHook {
    exclusions: Arc<RwLock<HashMap<Entity, u32>>>,
}

impl ExclusionHook {
    /// Creates a new ExclusionHook with the shared map.
    pub fn new(exclusions: Arc<RwLock<HashMap<Entity, u32>>>) -> Self {
        Self { exclusions }
    }
}

impl PhysicsHooks for ExclusionHook {
    fn filter_contact_pair(&self, context: &PairFilterContext) -> Option<SolverFlags> {
        let c1 = context.colliders.get(context.collider1)?;
        let c2 = context.colliders.get(context.collider2)?;

        // Bevy Rapier stores Entity bits in user_data (u128)
        let e1 = Entity::from_bits(c1.user_data as u64);
        let e2 = Entity::from_bits(c2.user_data as u64);

        if let Ok(map) = self.exclusions.read() {
            if let (Some(&g1), Some(&g2)) = (map.get(&e1), map.get(&e2)) {
                if g1 == g2 {
                    // Disable collision response
                    return Some(SolverFlags::empty());
                }
            }
        }

        None
    }

    fn filter_intersection_pair(&self, _context: &PairFilterContext) -> bool {
        false
    }

    fn modify_solver_contacts(&self, _context: &mut ContactModificationContext) {
    }
}

/// Syncs `PhysicsLayers` component changes to Rapier's `CollisionGroups`.
pub fn sync_physics_layers(
    mut commands: Commands,
    query: Query<(Entity, &PhysicsLayers), Changed<PhysicsLayers>>,
) {
    for (entity, layers) in query.iter() {
        commands.entity(entity).insert(CollisionGroups::new(
            Group::from_bits_truncate(layers.0),
            Group::from_bits_truncate(layers.0),
        ));
    }
}

/// Syncs `DisableSelfCollision` component changes to the shared `CollisionExclusionMap`.
pub fn sync_collision_exclusions(
    query: Query<(Entity, &DisableSelfCollision), Changed<DisableSelfCollision>>,
    mut removed: RemovedComponents<DisableSelfCollision>,
    map_res: Res<CollisionExclusionMap>,
) {
    if let Ok(mut map) = map_res.map.write() {
        for (entity, exclusion) in query.iter() {
            map.insert(entity, exclusion.0);
        }
        for entity in removed.read() {
            map.remove(&entity);
        }
    }
}
