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

/// A world-space contact sample: the point, its unit normal, and the normal
/// impulse. Divide the impulse by the timestep for the contact force.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactSample {
    /// World-space contact point.
    pub point: Vec2,
    /// Unit contact normal (world space, from the first shape toward the second).
    pub normal: Vec2,
    /// Normal impulse magnitude (N·s); force ≈ impulse / dt.
    pub normal_impulse: f32,
}

/// Read-only spatial queries against the physics world.
#[derive(SystemParam)]
pub struct PhysicsQueries<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    velocities: Query<'w, 's, (&'static LinearVelocity, &'static AngularVelocity)>,
    masses: Query<'w, 's, &'static ComputedMass>,
    sleeping: Query<'w, 's, Has<Sleeping>>,
    contacts: Res<'w, ContactGraph>,
}

impl PhysicsQueries<'_, '_> {
    /// Every touching contact point in the world (active + sleeping). The read
    /// facade for contact overlays and for scripts/plotters introspecting
    /// contact forces — keeping this here is what makes those a *read*, not a
    /// new physics-coupled path (`docs/script-lisp-decision.md`).
    pub fn contact_points(&self) -> Vec<ContactSample> {
        let mut samples = Vec::new();
        for pair in self
            .contacts
            .iter_active_touching()
            .chain(self.contacts.iter_sleeping_touching())
        {
            for manifold in &pair.manifolds {
                for point in &manifold.points {
                    samples.push(ContactSample {
                        point: point.point,
                        normal: manifold.normal,
                        normal_impulse: point.normal_impulse,
                    });
                }
            }
        }
        samples
    }

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

    /// The solver's computed mass, if the body simulates.
    pub fn mass_of(&self, entity: Entity) -> Option<f32> {
        self.masses.get(entity).ok().map(|m| m.value())
    }

    /// Net normal-contact impulse on `entity` over the last physics step
    /// (world space; force ≈ impulse / fixed dt). A box resting on the
    /// floor reads its weight pointing up. Probes and the plot panel read
    /// contact force through this, keeping the facade complete for
    /// scripts/plotters.
    ///
    /// avian 0.7's `ContactPoint::normal_impulse` accumulates the running
    /// impulse once per solver pass (bias solve + relaxation), which is 2×
    /// the physical step impulse at steady state — halved here so the read
    /// is in real N·s (a resting body reports ≈ its weight × dt).
    pub fn net_contact_impulse(&self, entity: Entity) -> Vec2 {
        const SOLVER_PASSES: f32 = 2.0;
        let mut total = Vec2::ZERO;
        for pair in self
            .contacts
            .iter_active_touching()
            .chain(self.contacts.iter_sleeping_touching())
        {
            // The manifold normal points from the first shape toward the
            // second; the separating impulse pushes body 1 against it.
            let sign = if pair.collider1 == entity || pair.body1 == Some(entity) {
                -1.0
            } else if pair.collider2 == entity || pair.body2 == Some(entity) {
                1.0
            } else {
                continue;
            };
            for manifold in &pair.manifolds {
                for point in &manifold.points {
                    total += manifold.normal * point.normal_impulse * sign;
                }
            }
        }
        total / SOLVER_PASSES
    }
}
