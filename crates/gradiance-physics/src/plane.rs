//! Holding bodies on their simulation plane — and why that is nearly free.
//!
//! Gradiance authors in 2D on a 3D engine. Every body is confined to its
//! simulation plane, which is what lets a mechanism be built by 2D drags with
//! no degree-of-freedom decisions: the plane *is* the constraint.
//!
//! # Locked axes, not a joint
//!
//! rapier implements [`LockedAxes`] by zeroing the corresponding entries of a
//! body's inverse mass and inertia tensors. The locked degrees of freedom stop
//! existing — there is **no constraint row**, no solver iteration spent on
//! them, and nothing for the solver to violate. A coplanar island's effective
//! problem is `3n` rather than `6n`.
//!
//! The alternative — a `GenericJoint` from every body to a plane anchor —
//! costs three constraint rows per body and can drift. It buys one thing the
//! locks cannot: a plane that *moves*, or one not aligned to a world axis
//! (rapier's locks are world-axis). That is the future, and
//! [`PlaneConstraint`] is where it arrives; today every plane is the world XY
//! plane and every body takes the free path.
//!
//! This is the performance argument for the whole 2.5D-on-3D design, and it is
//! what offsets the one real cost of the move: coplanar contact manifolds in 3D
//! carry more points than their 2D equivalents.

use bevy_rapier3d::prelude::*;
use gradiance_core::units::PlaneFrame;
use gradiance_domain::props::BodyPhysics;

/// How a body is held on its simulation plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneConstraint {
    /// Free: the out-of-plane degrees of freedom are removed from the body's
    /// mass properties, costing the solver nothing. Requires a plane aligned
    /// to a world axis and standing still — which is every plane today.
    Locked(LockedAxes),
    /// Constrained by a joint to a plane anchor. Costs three rows per body;
    /// the only option for a moving or tilted plane. Not built yet.
    Jointed,
}

/// How to hold a body on `frame`.
///
/// Always [`PlaneConstraint::Locked`] today. A moving plane returns
/// [`PlaneConstraint::Jointed`] here and nowhere else changes.
#[must_use]
pub fn plane_constraint(frame: &PlaneFrame) -> PlaneConstraint {
    debug_assert!(
        frame.normal().abs().max_element() > 0.999,
        "a non-axis-aligned plane needs PlaneConstraint::Jointed",
    );
    PlaneConstraint::Locked(
        LockedAxes::TRANSLATION_LOCKED_Z
            | LockedAxes::ROTATION_LOCKED_X
            | LockedAxes::ROTATION_LOCKED_Y,
    )
}

/// **The single producer of [`LockedAxes`] in the codebase.**
///
/// Two independent concerns meet here and neither may clobber the other: the
/// simulation-plane constraint (derived, never authored) and the body's
/// authored `rotation_locked` flag. Composing them anywhere else would mean one
/// write silently dropping the other's bits — which is exactly what used to
/// happen when `rotation_locked` *was* the presence of a `LockedAxes`
/// component.
#[must_use]
pub fn locked_axes(constraint: PlaneConstraint, physics: &BodyPhysics) -> LockedAxes {
    let mut axes = match constraint {
        PlaneConstraint::Locked(plane) => plane,
        // A jointed plane leaves the out-of-plane freedoms to the joint.
        PlaneConstraint::Jointed => LockedAxes::empty(),
    };
    if physics.rotation_locked {
        // In-plane rotation is about the plane normal, which for the world XY
        // plane is Z. A tilted plane locks its own normal via the joint above.
        axes |= LockedAxes::ROTATION_LOCKED_Z;
    }
    axes
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_domain::props::BodyKind;

    fn unlocked() -> BodyPhysics {
        BodyPhysics::default()
    }

    fn rotation_locked() -> BodyPhysics {
        BodyPhysics {
            rotation_locked: true,
            ..BodyPhysics::default()
        }
    }

    #[test]
    fn the_default_plane_is_held_by_free_locks() {
        let PlaneConstraint::Locked(axes) = plane_constraint(&PlaneFrame::XY) else {
            panic!("the world XY plane needs no joint");
        };
        assert!(axes.contains(LockedAxes::TRANSLATION_LOCKED_Z));
        assert!(axes.contains(LockedAxes::ROTATION_LOCKED_X));
        assert!(axes.contains(LockedAxes::ROTATION_LOCKED_Y));
        // In-plane freedoms survive: a body still slides and spins in plane.
        assert!(!axes.contains(LockedAxes::TRANSLATION_LOCKED_X));
        assert!(!axes.contains(LockedAxes::TRANSLATION_LOCKED_Y));
        assert!(!axes.contains(LockedAxes::ROTATION_LOCKED_Z));
    }

    #[test]
    fn plane_lock_and_rotation_lock_compose() {
        let constraint = plane_constraint(&PlaneFrame::XY);
        let plane_bits = LockedAxes::TRANSLATION_LOCKED_Z
            | LockedAxes::ROTATION_LOCKED_X
            | LockedAxes::ROTATION_LOCKED_Y;

        // Toggling the authored flag must never disturb the plane's bits.
        for _ in 0..10 {
            let free = locked_axes(constraint, &unlocked());
            assert!(free.contains(plane_bits), "plane lock survived");
            assert!(!free.contains(LockedAxes::ROTATION_LOCKED_Z));

            let held = locked_axes(constraint, &rotation_locked());
            assert!(held.contains(plane_bits), "plane lock survived");
            assert!(held.contains(LockedAxes::ROTATION_LOCKED_Z));
        }
    }

    #[test]
    fn the_body_kind_does_not_affect_the_plane_lock() {
        let constraint = plane_constraint(&PlaneFrame::XY);
        for kind in [BodyKind::Dynamic, BodyKind::Static, BodyKind::Kinematic] {
            let axes = locked_axes(
                constraint,
                &BodyPhysics {
                    kind,
                    ..BodyPhysics::default()
                },
            );
            assert!(axes.contains(LockedAxes::TRANSLATION_LOCKED_Z));
        }
    }

    #[test]
    fn a_jointed_plane_leaves_the_out_of_plane_freedoms_alone() {
        // The seam a moving plane arrives at: the joint owns the plane, the
        // authored flag still owns in-plane rotation.
        let axes = locked_axes(PlaneConstraint::Jointed, &rotation_locked());
        assert_eq!(axes, LockedAxes::ROTATION_LOCKED_Z);
        assert!(locked_axes(PlaneConstraint::Jointed, &unlocked()).is_empty());
    }
}
