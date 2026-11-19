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
use gradiance_core::ids::IdIndex;
use gradiance_core::units::PosRot;
use gradiance_domain::joint::{JointDef, JointSensors, WORLD_POSE};
use gradiance_domain::quantity::{self, BodyRead, JointRead};
use gradiance_units::{
    Angle, AngularMomentum, AngularVelocity, Energy, Force, Force2, Impulse, Impulse2, Length,
    Mass, Momentum, Velocity2,
};

/// A world-space contact sample: the point, its unit normal, and the normal
/// impulse. Divide the impulse by the timestep for the contact force.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactSample {
    /// World-space contact point.
    pub point: Vec2,
    /// Unit contact normal (world space, from the first shape toward the
    /// second) — a dimensionless direction, so it stays a bare `Vec2`.
    pub normal: Vec2,
    /// Normal impulse magnitude; force ≈ impulse / dt.
    pub normal_impulse: Impulse,
}

/// Read-only spatial queries against the physics world.
#[derive(SystemParam)]
pub struct PhysicsQueries<'w, 's> {
    /// Colliders keyed by their **live** editor `Transform` — the hit-test
    /// facade reads these directly rather than avian's spatial BVH, which is
    /// frozen while the sim is paused (see [`bodies_at_point`](Self::bodies_at_point)).
    colliders: Query<'w, 's, (Entity, &'static Transform, &'static Collider)>,
    // avian's `AngularVelocity` (the component) is name-shadowed by the units
    // quantity imported above, so qualify it here; the read returns the typed
    // quantity, not the component.
    velocities: Query<
        'w,
        's,
        (
            &'static LinearVelocity,
            &'static avian2d::prelude::AngularVelocity,
        ),
    >,
    masses: Query<'w, 's, &'static ComputedMass>,
    inertias: Query<'w, 's, &'static ComputedAngularInertia>,
    sleeping: Query<'w, 's, Has<Sleeping>>,
    contacts: Res<'w, ContactGraph>,
    /// Authored joint definitions — read for the joint sensor reads; body
    /// poses come from `colliders` (a body's collider is on the body entity).
    joints: Query<'w, 's, &'static JointDef>,
    /// avian's post-solve reaction on each live joint (`joint_sync` inserts
    /// [`JointForces`] on every derived joint) and whether it is disabled.
    joint_forces: Query<'w, 's, (&'static JointForces, Has<JointDisabled>)>,
    /// The live engine joints, for their derived motor ceilings.
    engine_joints: Query<
        'w,
        's,
        (
            Option<&'static RevoluteJoint>,
            Option<&'static PrismaticJoint>,
        ),
    >,
}

impl<'w, 's> PhysicsQueries<'w, 's> {
    /// One body as the quantity catalog reads it ([`BodyRead`]). `position`
    /// is the body's world position, read by the caller (from its
    /// `Transform`), and `dt` the fixed timestep the contact impulse is
    /// divided by. Nothing is read until a reader asks.
    pub fn body(&self, entity: Entity, position: Option<Vec2>, dt: f32) -> LiveBody<'_, 'w, 's> {
        LiveBody {
            physics: self,
            entity,
            position,
            dt,
        }
    }

    /// One joint as the quantity catalog reads it ([`JointRead`]); `index`
    /// resolves the bodies it connects.
    pub fn joint<'a>(&'a self, entity: Entity, index: &'a IdIndex) -> LiveJoint<'a, 'w, 's> {
        LiveJoint {
            physics: self,
            entity,
            index,
        }
    }

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
                        normal_impulse: Impulse(point.normal_impulse),
                    });
                }
            }
        }
        samples
    }

    /// All collider entities containing `point`, unordered.
    ///
    /// Iterates colliders against their **live** [`Transform`] and tests
    /// `contains_point` exactly, rather than avian's spatial BVH. The BVH's
    /// broad phase runs inside the physics step, which is frozen while the sim
    /// is paused — so a body moved while paused stays at its old BVH slot and
    /// becomes unpickable until the next step (the "objects go non-selectable
    /// until I play/pause" bug). The editor has few bodies and this runs only
    /// on a click / box gesture, so the linear scan is cheap and always fresh.
    pub fn bodies_at_point(&self, point: Vec2) -> Vec<Entity> {
        self.colliders
            .iter()
            .filter_map(|(entity, transform, collider)| {
                let pose = PosRot::from_transform(transform);
                collider
                    .contains_point(pose.pos, Rotation::radians(pose.rot), point)
                    .then_some(entity)
            })
            .collect()
    }

    /// All collider entities whose live AABB intersects the given box, unordered.
    /// Uses the live [`Transform`] for the same paused-freshness reason as
    /// [`bodies_at_point`](Self::bodies_at_point).
    pub fn bodies_in_aabb(&self, min: Vec2, max: Vec2) -> Vec<Entity> {
        let box_aabb = ColliderAabb::from_min_max(min, max);
        self.colliders
            .iter()
            .filter_map(|(entity, transform, collider)| {
                let pose = PosRot::from_transform(transform);
                collider
                    .aabb(pose.pos, Rotation::radians(pose.rot), 0.0)
                    .intersects(&box_aabb)
                    .then_some(entity)
            })
            .collect()
    }

    /// A joint's live sensor state (length between its world anchors, angle
    /// in the hinge-limit frame) — the read behind the `JointLength` /
    /// `JointAngle` signal sources. Derived from the bodies' live poses and
    /// the authored [`JointDef`] alone (`JointDef::sensor_state`), never from
    /// avian's joint entity. A world pin measures against its world point.
    /// `None` if `joint` isn't a joint or an endpoint body doesn't resolve.
    pub fn joint_state(&self, joint: Entity, index: &IdIndex) -> Option<JointSensors> {
        let def = self.joints.get(joint).ok()?;
        let pose = |id| {
            let (_, transform, _) = self.colliders.get(index.entity(id)?).ok()?;
            Some(PosRot::from_transform(transform))
        };
        let pose_a = pose(def.body_a)?;
        let pose_b = match def.body_b {
            Some(id) => pose(id)?,
            None => WORLD_POSE,
        };
        Some(def.sensor_state(pose_a, pose_b))
    }

    /// Distance between a joint's two world anchors (see
    /// [`joint_state`](Self::joint_state)).
    pub fn joint_length(&self, joint: Entity, index: &IdIndex) -> Option<Length> {
        self.joint_state(joint, index).map(|s| s.length)
    }

    /// A joint's relative rotation minus its authored rest angle (see
    /// [`joint_state`](Self::joint_state)).
    pub fn joint_angle(&self, joint: Entity, index: &IdIndex) -> Option<Angle> {
        self.joint_state(joint, index).map(|s| s.angle)
    }

    /// Magnitude of a joint's **linear** reaction force after the last solve
    /// — the read behind the `JointForce` signal source and the quantity a
    /// `break_force` is compared against. A disabled (broken) joint exerts
    /// nothing and reads zero; avian keeps writing its last solve's stale
    /// multipliers, so the zero is applied here rather than trusted from the
    /// component. `None` if `joint` has no live derived joint.
    pub fn joint_force(&self, joint: Entity) -> Option<Force> {
        let (forces, disabled) = self.joint_forces.get(joint).ok()?;
        Some(Force(if disabled {
            0.0
        } else {
            forces.force().length()
        }))
    }

    /// The torque (N·m, hinge) or force (N, slider) ceiling the physics seam
    /// derived for a joint's motor — the authored cap, or for an **auto**
    /// motor the one scaled to the connected bodies
    /// (`joint_sync::retune_joints`). `None` if `joint` has no live hinge or
    /// slider (a spring, or not derived yet).
    pub fn motor_ceiling(&self, joint: Entity) -> Option<f32> {
        match self.engine_joints.get(joint).ok()? {
            (Some(revolute), _) => Some(revolute.motor.max_torque),
            (_, Some(prismatic)) => Some(prismatic.motor.max_force),
            _ => None,
        }
    }

    /// The entity's `(linear, angular)` velocity, if it simulates.
    pub fn velocity_of(&self, entity: Entity) -> Option<(Velocity2, AngularVelocity)> {
        self.velocities
            .get(entity)
            .ok()
            .map(|(lin, ang)| (Velocity2::new(lin.0), AngularVelocity(ang.0)))
    }

    /// Whether the body is asleep (solver has parked it).
    pub fn is_sleeping(&self, entity: Entity) -> bool {
        self.sleeping.get(entity).unwrap_or(false)
    }

    /// The solver's computed mass, if the body simulates.
    pub fn mass_of(&self, entity: Entity) -> Option<Mass> {
        self.masses.get(entity).ok().map(|m| Mass(m.value()))
    }

    /// The solver's computed angular inertia (moment of inertia, kg·m²), if the
    /// body simulates — the rotational analogue of [`mass_of`](Self::mass_of),
    /// for rotational kinetic energy `½Iω²`.
    pub fn angular_inertia_of(&self, entity: Entity) -> Option<f32> {
        self.inertias.get(entity).ok().map(|i| i.value())
    }

    /// Total kinetic energy `½mv² + ½Iω²` of a simulating body — a derived read
    /// on the query surface; the formula is the quantity catalog's
    /// ([`quantity::kinetic_energy`]), so probes, plotters, and scripts share
    /// one definition (`docs/units-decision.md`).
    pub fn kinetic_energy_of(&self, entity: Entity) -> Option<Energy> {
        let mass = self.mass_of(entity)?;
        let (lin, ang) = self.velocity_of(entity)?;
        Some(quantity::kinetic_energy(
            mass,
            self.angular_inertia_of(entity),
            lin,
            ang,
        ))
    }

    /// Linear momentum magnitude `m·|v|` of a simulating body (the other
    /// conserved quantity beside [`kinetic_energy_of`](Self::kinetic_energy_of)).
    pub fn momentum_of(&self, entity: Entity) -> Option<Momentum> {
        let mass = self.mass_of(entity)?;
        let (lin, _) = self.velocity_of(entity)?;
        Some(quantity::momentum(mass, lin))
    }

    /// Angular momentum `I·ω` of a simulating body — the rotational conserved
    /// quantity completing the trio with [`kinetic_energy_of`](Self::kinetic_energy_of)
    /// and [`momentum_of`](Self::momentum_of). Signed by `ω` (CCW positive).
    pub fn angular_momentum_of(&self, entity: Entity) -> Option<AngularMomentum> {
        let inertia = self.angular_inertia_of(entity)?;
        let (_, ang) = self.velocity_of(entity)?;
        Some(quantity::angular_momentum(inertia, ang))
    }

    /// How many distinct bodies `entity` is currently touching (contact
    /// pairs, active + sleeping). Probe/signal read — e.g. "color a body
    /// by how many things it rests on".
    pub fn touching_count(&self, entity: Entity) -> usize {
        self.contacts
            .iter_active_touching()
            .chain(self.contacts.iter_sleeping_touching())
            .filter(|pair| {
                pair.collider1 == entity
                    || pair.collider2 == entity
                    || pair.body1 == Some(entity)
                    || pair.body2 == Some(entity)
            })
            .count()
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
    pub fn net_contact_impulse(&self, entity: Entity) -> Impulse2 {
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
        Impulse2::new(total / SOLVER_PASSES)
    }
}

/// A body read through the facade: the live [`BodyRead`] the quantity
/// catalog's readers run on in the frame loop. Built by
/// [`PhysicsQueries::body`]; each method is one facade read.
pub struct LiveBody<'a, 'w, 's> {
    physics: &'a PhysicsQueries<'w, 's>,
    entity: Entity,
    position: Option<Vec2>,
    dt: f32,
}

impl BodyRead for LiveBody<'_, '_, '_> {
    fn position(&self) -> Option<Vec2> {
        self.position
    }

    fn velocity(&self) -> Option<(Velocity2, AngularVelocity)> {
        self.physics.velocity_of(self.entity)
    }

    fn mass(&self) -> Option<Mass> {
        self.physics.mass_of(self.entity)
    }

    fn angular_inertia(&self) -> Option<f32> {
        self.physics.angular_inertia_of(self.entity)
    }

    fn contact_force(&self) -> Option<Force2> {
        let impulse = self.physics.net_contact_impulse(self.entity).value();
        Some(Force2::new(impulse / self.dt.max(1e-6)))
    }

    fn contact_count(&self) -> Option<usize> {
        Some(self.physics.touching_count(self.entity))
    }
}

/// A joint read through the facade (see [`LiveBody`]).
pub struct LiveJoint<'a, 'w, 's> {
    physics: &'a PhysicsQueries<'w, 's>,
    entity: Entity,
    index: &'a IdIndex,
}

impl JointRead for LiveJoint<'_, '_, '_> {
    fn sensors(&self) -> Option<JointSensors> {
        self.physics.joint_state(self.entity, self.index)
    }

    fn reaction_force(&self) -> Option<Force> {
        self.physics.joint_force(self.entity)
    }
}
