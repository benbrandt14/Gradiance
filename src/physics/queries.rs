//! Engine-agnostic spatial query facade.
//!
//! Tools and interaction code use this instead of avian types, keeping the
//! engine swap seam intact. Everything is expressed in world-space `Vec2`
//! and plain `Entity` lists.

use avian2d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// Read-only spatial queries against the physics world.
#[derive(SystemParam)]
pub struct PhysicsQueries<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
}

impl PhysicsQueries<'_, '_> {
    /// All collider entities containing `point`, unordered.
    pub fn bodies_at_point(&self, point: Vec2) -> Vec<Entity> {
        self.spatial
            .point_intersections(point, &SpatialQueryFilter::default())
    }

    /// All collider entities whose AABB intersects the given box, unordered.
    pub fn bodies_in_aabb(&self, min: Vec2, max: Vec2) -> Vec<Entity> {
        self.spatial
            .aabb_intersections_with_aabb(ColliderAabb::from_min_max(min, max))
    }
}
