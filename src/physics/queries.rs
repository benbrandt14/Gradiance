//! The physics read cut-point: one thin, testable facade over avian's
//! spatial/velocity queries.
//!
//! Post de-adapter (`docs/physics-deadapter-decision.md`) this is a
//! convenience layer, not an abstraction boundary — consumers *may* read avian
//! components directly (and the scripting reflection bridge does). It survives
//! because it keeps common reads in one discoverable, unit-testable place and
//! returns plain `Vec2`/`Entity`, which is what most callers want.

use avian2d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// Read-only spatial queries against the physics world.
#[derive(SystemParam)]
pub struct PhysicsQueries<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    velocities: Query<'w, 's, (&'static LinearVelocity, &'static AngularVelocity)>,
    sleeping: Query<'w, 's, Has<Sleeping>>,
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

    /// The entity's `(linear, angular)` velocity, if it simulates.
    pub fn velocity_of(&self, entity: Entity) -> Option<(Vec2, f32)> {
        self.velocities
            .get(entity)
            .ok()
            .map(|(lin, ang)| (lin.0, ang.0))
    }

    /// Whether the body is asleep (solver has parked it).
    pub fn is_sleeping(&self, entity: Entity) -> bool {
        self.sleeping.get(entity).unwrap_or(false)
    }
}
