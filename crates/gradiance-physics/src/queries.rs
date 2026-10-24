//! The physics read cut-point: one thin, testable facade over the engine's
//! spatial and dynamic queries.
//!
//! Reads are total (`docs/script-lisp-decision.md`): probes, plotters, the
//! contact overlay and scripts all come through here, so a new physical
//! quantity becomes available everywhere at once. This is a convenience and DRY
//! layer, not an abstraction boundary — but it *is* where three dimensions are
//! projected back to the plane-local, typed quantities every caller wants.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_rapier3d::plugin::ReadRapierContext;
use bevy_rapier3d::prelude::*;
use gradiance_core::units::PlaneFrame;
use gradiance_units::{
    AngularMomentum, AngularVelocity, Energy, Impulse, Impulse2, Mass, MomentOfInertia, Momentum,
    Velocity2,
};

/// A world-space contact sample: the point, its unit normal, and the normal
/// impulse. Divide the impulse by the timestep for the contact force.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactSample {
    /// Plane-local contact point.
    pub point: Vec2,
    /// Unit contact normal, from the first shape toward the second — a
    /// dimensionless direction, so it stays a bare `Vec2`.
    ///
    /// Projected from the engine's 3D normal. That projection is lossless in
    /// practice rather than merely convenient: every collider spans the same
    /// slab in z (see `body_sync::SLAB`), so the plane normal is never a
    /// separating axis and every manifold normal is already in-plane.
    pub normal: Vec2,
    /// Normal impulse magnitude; force ≈ impulse / dt.
    pub normal_impulse: Impulse,
}

/// Read-only queries against the physics world.
#[derive(SystemParam)]
pub struct PhysicsQueries<'w, 's> {
    context: ReadRapierContext<'w, 's>,
    /// Colliders keyed by their **live** editor `Transform` — the hit-test
    /// facade reads these directly rather than the engine's broad-phase tree,
    /// which is not refit while the simulation is paused (see
    /// [`bodies_at_point`](Self::bodies_at_point)).
    colliders: Query<'w, 's, (Entity, &'static Transform, &'static Collider)>,
    velocities: Query<'w, 's, &'static Velocity>,
    masses: Query<'w, 's, &'static ReadMassProperties>,
    sleeping: Query<'w, 's, &'static Sleeping>,
    bodies: Query<'w, 's, Entity, With<RigidBody>>,
    timestep: Res<'w, TimestepMode>,
}

impl PhysicsQueries<'_, '_> {
    /// The simulation plane every read is projected onto.
    ///
    /// One plane today; a second is a second value here and nowhere else.
    fn plane() -> PlaneFrame {
        PlaneFrame::XY
    }

    /// The physics world these reads run against.
    ///
    /// One world today; a second simulation plane is a second context, which is
    /// why this is a lookup rather than a field.
    fn world(&self) -> Option<bevy_rapier3d::plugin::RapierContext<'_>> {
        self.context.single().ok()
    }

    /// Every touching contact point in the world. The read facade for contact
    /// overlays and for scripts introspecting contact forces.
    pub fn contact_points(&self) -> Vec<ContactSample> {
        let plane = Self::plane();
        let mut samples = Vec::new();
        let mut seen: Vec<(Entity, Entity)> = Vec::new();
        let Some(world) = self.world() else {
            return samples;
        };
        for entity in &self.bodies {
            for pair in world.contact_pairs_with(entity) {
                let (Some(a), Some(b)) = (pair.collider1(), pair.collider2()) else {
                    continue;
                };
                let key = if a < b { (a, b) } else { (b, a) };
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                // Contact points are local to the *first* collider; lift them
                // into the world before projecting, or every contact reads as
                // an offset from the body's own origin.
                let Ok((_, transform, _)) = self.colliders.get(a) else {
                    continue;
                };
                for manifold in pair.manifolds() {
                    let normal = plane.project_dir(manifold.normal()).normalize_or_zero();
                    for point in manifold.points() {
                        if point.dist() > 0.0 {
                            continue;
                        }
                        let world = transform.transform_point(point.local_p1());
                        samples.push(ContactSample {
                            point: plane.project(world).0,
                            normal,
                            normal_impulse: Impulse(point.impulse()),
                        });
                    }
                }
            }
        }
        samples
    }

    /// All collider entities containing `point`, unordered.
    ///
    /// Iterates colliders against their **live** [`Transform`] rather than the
    /// broad-phase tree. That tree is refit inside the physics step, which is
    /// frozen while the simulation is paused — so a body moved while paused
    /// would keep its old tree slot and become unpickable until the next step
    /// (the "objects go non-selectable until I play/pause" bug). The editor has
    /// few bodies and this runs only on a click or box gesture, so the linear
    /// scan is cheap and always fresh.
    pub fn bodies_at_point(&self, point: Vec2) -> Vec<Entity> {
        let plane = Self::plane();
        let probe = plane.point(point, 0.0);
        self.colliders
            .iter()
            .filter_map(|(entity, transform, collider)| {
                collider
                    .contains_point(transform.translation, transform.rotation, probe)
                    .then_some(entity)
            })
            .collect()
    }

    /// All collider entities whose live bounding box intersects the given box,
    /// unordered. Uses the live [`Transform`] for the same paused-freshness
    /// reason as [`bodies_at_point`](Self::bodies_at_point).
    pub fn bodies_in_aabb(&self, min: Vec2, max: Vec2) -> Vec<Entity> {
        let plane = Self::plane();
        self.colliders
            .iter()
            .filter_map(|(entity, transform, collider)| {
                let mut pose = bevy_rapier3d::parry::math::Pose::from_rotation(transform.rotation);
                pose.translation = transform.translation;
                let aabb = collider.raw.compute_aabb(&pose);
                let lo = plane.project(aabb.mins).0;
                let hi = plane.project(aabb.maxs).0;
                (hi.x >= min.x && lo.x <= max.x && hi.y >= min.y && lo.y <= max.y).then_some(entity)
            })
            .collect()
    }

    /// The entity's `(linear, angular)` velocity, if it simulates.
    pub fn velocity_of(&self, entity: Entity) -> Option<(Velocity2, AngularVelocity)> {
        let plane = Self::plane();
        self.velocities.get(entity).ok().map(|v| {
            (
                Velocity2::new(plane.project_dir(v.linear)),
                AngularVelocity(plane.unspin(v.angular)),
            )
        })
    }

    /// Whether the body is asleep (the solver has parked it).
    pub fn is_sleeping(&self, entity: Entity) -> bool {
        self.sleeping.get(entity).is_ok_and(|s| s.sleeping)
    }

    /// The solver's computed mass, if the body simulates.
    pub fn mass_of(&self, entity: Entity) -> Option<Mass> {
        self.masses.get(entity).ok().map(|m| Mass(m.get().mass))
    }

    /// The solver's moment of inertia about the plane normal — the rotational
    /// analogue of [`mass_of`](Self::mass_of), for rotational kinetic energy.
    pub fn angular_inertia_of(&self, entity: Entity) -> Option<MomentOfInertia> {
        let plane = Self::plane();
        self.masses
            .get(entity)
            .ok()
            .map(|m| MomentOfInertia(plane.unspin(m.get().principal_inertia).abs()))
    }

    /// Total kinetic energy `½mv² + ½Iω²` of a simulating body — a derived read
    /// on the query surface, so probes, plotters and scripts share one
    /// definition.
    pub fn kinetic_energy_of(&self, entity: Entity) -> Option<Energy> {
        let mass = self.mass_of(entity)?;
        let (linear, angular) = self.velocity_of(entity)?;
        let translational = 0.5 * mass.value() * linear.value().length_squared();
        let rotational = self
            .angular_inertia_of(entity)
            .map_or(0.0, |i| 0.5 * i.value() * angular.value().powi(2));
        Some(Energy(translational + rotational))
    }

    /// Linear momentum magnitude `m·|v|` of a simulating body.
    pub fn momentum_of(&self, entity: Entity) -> Option<Momentum> {
        let mass = self.mass_of(entity)?;
        let (linear, _) = self.velocity_of(entity)?;
        Some(Momentum(mass.value() * linear.value().length()))
    }

    /// Angular momentum `I·ω` of a simulating body, signed by `ω` (CCW
    /// positive).
    pub fn angular_momentum_of(&self, entity: Entity) -> Option<AngularMomentum> {
        let inertia = self.angular_inertia_of(entity)?;
        let (_, angular) = self.velocity_of(entity)?;
        Some(inertia * angular)
    }

    /// How many distinct bodies `entity` is currently touching. Probe/signal
    /// read — e.g. "color a body by how many things it rests on".
    pub fn touching_count(&self, entity: Entity) -> usize {
        self.world().map_or(0, |world| {
            world
                .contact_pairs_with(entity)
                .filter(bevy_rapier3d::plugin::ContactPairView::has_any_active_contact)
                .count()
        })
    }

    /// Net normal-contact impulse on `entity` over the last physics step
    /// (plane-local; force ≈ impulse / fixed dt). A box resting on the floor
    /// reads its weight pointing up.
    ///
    /// Both engines report a contact impulse that is not the step's, and each
    /// needs its own correction. The 2D engine accumulated once per solver
    /// pass, so the read halved it. rapier solves `substeps` sub-iterations per
    /// step and reports the impulse of one of them, so the read scales *up* by
    /// the substep count. `a_resting_body_reports_about_its_weight` is the
    /// calibration: it was written against the old engine precisely so this
    /// factor could be checked rather than guessed.
    pub fn net_contact_impulse(&self, entity: Entity) -> Impulse2 {
        let plane = Self::plane();
        let mut total = Vec2::ZERO;
        let Some(world) = self.world() else {
            return Impulse2::new(total);
        };
        for pair in world.contact_pairs_with(entity) {
            let (Some(a), Some(b)) = (pair.collider1(), pair.collider2()) else {
                continue;
            };
            // The manifold normal points from the first shape toward the
            // second; the separating impulse pushes body 1 against it.
            let sign = if a == entity {
                -1.0
            } else if b == entity {
                1.0
            } else {
                continue;
            };
            for manifold in pair.manifolds() {
                let normal = plane.project_dir(manifold.normal());
                for point in manifold.points() {
                    total += normal * point.impulse() * sign;
                }
            }
        }
        Impulse2::new(total * self.substeps())
    }

    /// Solver sub-iterations per physics step — the factor between a reported
    /// contact impulse and the step's.
    fn substeps(&self) -> f32 {
        match *self.timestep {
            TimestepMode::Fixed { substeps, .. }
            | TimestepMode::Variable { substeps, .. }
            | TimestepMode::Interpolated { substeps, .. } => substeps.max(1) as f32,
        }
    }
}

/// Reads a body's plane-local motion directly from a `&World`.
///
/// The [`PhysicsQueries`] `SystemParam` is the facade for systems; this is the
/// same read for the exclusive-`World` callers that cannot hold one — commands.
/// Returns `None` for a body that does not simulate.
pub fn read_motion(world: &World, entity: Entity) -> Option<(Velocity2, AngularVelocity)> {
    let plane = PlaneFrame::XY;
    let velocity = world.get::<Velocity>(entity)?;
    Some((
        Velocity2::new(plane.project_dir(velocity.linear)),
        AngularVelocity(plane.unspin(velocity.angular)),
    ))
}

/// Writes a body's plane-local motion.
///
/// Velocity is simulation state, never authored state: this exists so a command
/// that *destroys and respawns* a body can carry its motion across (the cut
/// tool's `v + ω × r` split), not so commands can drive physics. It is never
/// recorded and never undone — the same status as the grab spring's writes.
pub fn write_motion(
    world: &mut World,
    entity: Entity,
    linear: Velocity2,
    angular: AngularVelocity,
) {
    let plane = PlaneFrame::XY;
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.insert(Velocity {
            linear: plane.dir(linear.value()),
            angular: plane.spin(angular.value()),
        });
    }
}
