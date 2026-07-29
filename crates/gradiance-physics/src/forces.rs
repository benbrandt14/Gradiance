//! The per-step force seam: contributors accumulate, one system commits.
//!
//! Fields, plane friction and the twist gesture all push on bodies, and all of
//! them are **one-shot**: whatever they applied this step is gone the next, so
//! the solver and its constraints always have the last word. avian expresses
//! that directly (`Forces` clears after the step). rapier does not — its
//! `ExternalForce` is *persistent*, re-applied every step until someone changes
//! it.
//!
//! This module owns that difference. Contributors add plane-local typed
//! quantities here; [`commit_forces`] writes the total through and clears.
//! Nothing else may touch the engine's force components.
//!
//! # Why the commit is change-gated
//!
//! rapier re-applies `ExternalForce` when it is marked `Changed`, and marking
//! it **wakes the body**. A commit that writes unconditionally therefore wakes
//! every body every frame: sleeping never engages, islands never quiesce, and a
//! large scene burns the whole solver budget on bodies that are not moving.
//!
//! There is no error and no visible symptom — only "it is slow". Writing a
//! force only when it actually differs is what keeps sleeping working, so the
//! gate is load-bearing rather than an optimization, and
//! `sleeping_engages_with_no_active_forces` guards it.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use gradiance_units::{Force2, Torque};

/// Accumulated per-step forces and torques, keyed by body.
///
/// Plane-local and typed: contributors never see the engine's 3D form, which is
/// what lets the whole force path stay 2D above the sync seam.
#[derive(Resource, Default, Debug)]
pub struct ForceAccumulator {
    linear: HashMap<Entity, Force2>,
    angular: HashMap<Entity, Torque>,
}

impl ForceAccumulator {
    /// Adds a force on `entity` for this step.
    pub fn add_force(&mut self, entity: Entity, force: Force2) {
        *self.linear.entry(entity).or_default() += force;
    }

    /// Adds a torque about the plane normal on `entity` for this step.
    pub fn add_torque(&mut self, entity: Entity, torque: Torque) {
        *self.angular.entry(entity).or_default() += torque;
    }

    /// The accumulated force on `entity` — the field vector-plot overlay reads
    /// what was actually applied rather than re-deriving it.
    #[must_use]
    pub fn force_of(&self, entity: Entity) -> Force2 {
        self.linear.get(&entity).copied().unwrap_or_default()
    }

    /// The accumulated torque on `entity`.
    #[must_use]
    pub fn torque_of(&self, entity: Entity) -> Torque {
        self.angular.get(&entity).copied().unwrap_or_default()
    }

    /// Every body with a contribution this step.
    pub fn bodies(&self) -> impl Iterator<Item = Entity> + '_ {
        self.linear.keys().chain(self.angular.keys()).copied()
    }

    /// Drops every accumulated contribution, ending the step.
    pub fn clear(&mut self) {
        self.linear.clear();
        self.angular.clear();
    }

    /// Whether anything was accumulated this step.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.linear.is_empty() && self.angular.is_empty()
    }
}

/// Applies the accumulated forces to the engine and clears the accumulator.
///
/// Runs after every contributor and before the solver step. Under avian the
/// engine's own force components are already one-shot, so this simply hands the
/// totals over; the accumulator exists so that contributors are engine-free and
/// so the one-shot semantics survive an engine that does not provide them.
pub fn commit_forces(
    mut accumulator: ResMut<ForceAccumulator>,
    mut bodies: Query<(Entity, &mut bevy_rapier3d::prelude::ExternalForce)>,
) {
    let plane = gradiance_core::units::PlaneFrame::XY;
    // Every force component is visited, not just the ones with a contribution:
    // a body that *stopped* being pushed this step must be zeroed, or rapier
    // would keep re-applying last step's force forever.
    for (entity, mut external) in &mut bodies {
        let force = plane.dir(accumulator.force_of(entity).value());
        let torque = plane.spin(accumulator.torque_of(entity).value());
        // Change-gated: see the module docs. Writing an unchanged value marks
        // the component `Changed`, which wakes the body — every frame, forever.
        if external.force != force || external.torque != torque {
            external.force = force;
            external.torque = torque;
        }
    }
    accumulator.clear();
}

/// Ensures every body carries the engine components the rest of the crate
/// reads and writes through.
///
/// All three are **opt-in** in rapier and none is inserted by the sync systems:
/// `ExternalForce` is where [`commit_forces`] writes, `Velocity` is both the
/// grab spring's write target and every velocity read, and
/// `ReadMassProperties` is the solver's mass/inertia readback that fields,
/// friction, the twist servo, auto-sized motor ceilings and the query facade
/// all depend on. Without them those systems simply match no entities — no
/// error, just a world where nothing pushes anything and nothing appears to
/// move.
pub fn ensure_engine_components(
    mut commands: Commands,
    missing: Query<
        Entity,
        (
            With<gradiance_domain::Body>,
            Or<(
                Without<bevy_rapier3d::prelude::ExternalForce>,
                Without<bevy_rapier3d::prelude::ReadMassProperties>,
                Without<bevy_rapier3d::prelude::Velocity>,
                Without<bevy_rapier3d::prelude::Sleeping>,
            )>,
        ),
    >,
) {
    for entity in &missing {
        // `insert_if_new`, not `insert`: the query matches when *any* of these
        // is absent, and a plain insert would then reset the others. That would
        // silently zero a velocity another system had just seeded — which is
        // exactly what the cut tool does when it carries a severed body's motion
        // onto its pieces.
        commands.entity(entity).insert_if_new((
            bevy_rapier3d::prelude::ExternalForce::default(),
            bevy_rapier3d::prelude::ReadMassProperties::default(),
            bevy_rapier3d::prelude::Velocity::default(),
            bevy_rapier3d::prelude::Sleeping::disabled(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::entity::Entity;
    use bevy::platform::collections::HashSet;

    fn entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).expect("valid test entity id")
    }

    #[test]
    fn contributions_superpose() {
        let (mut acc, e) = (ForceAccumulator::default(), entity(1));
        acc.add_force(e, Force2::new(Vec2::new(3.0, 0.0)));
        acc.add_force(e, Force2::new(Vec2::new(-1.0, 4.0)));
        acc.add_torque(e, Torque(2.0));
        acc.add_torque(e, Torque(-0.5));
        assert_eq!(acc.force_of(e), Force2::new(Vec2::new(2.0, 4.0)));
        assert_eq!(acc.torque_of(e), Torque(1.5));
    }

    #[test]
    fn a_body_with_no_contribution_reads_zero() {
        let acc = ForceAccumulator::default();
        assert_eq!(acc.force_of(entity(7)), Force2::default());
        assert_eq!(acc.torque_of(entity(7)), Torque::default());
        assert!(acc.is_empty());
    }

    #[test]
    fn clearing_ends_the_step() {
        let (mut acc, e) = (ForceAccumulator::default(), entity(2));
        acc.add_force(e, Force2::new(Vec2::X));
        assert!(!acc.is_empty());
        acc.clear();
        assert!(acc.is_empty());
        assert_eq!(acc.force_of(e), Force2::default());
    }

    #[test]
    fn bodies_lists_every_contributor() {
        let mut acc = ForceAccumulator::default();
        acc.add_force(entity(1), Force2::new(Vec2::X));
        acc.add_torque(entity(2), Torque(1.0));
        // A set, not a sequence: `bodies()` chains two hash-map key iterators,
        // and `Entity`'s own ordering is not index-ascending — depending on
        // either would be testing something this method does not promise.
        let seen: HashSet<Entity> = acc.bodies().collect();
        assert_eq!(seen, HashSet::from_iter([entity(1), entity(2)]));
    }
}
