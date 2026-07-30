//! The authored physics of a body — plain data, owned by the domain.
//!
//! [`BodyPhysics`] is a **live authored component** and the save content. The
//! physics layer translates it to engine components on `Changed<BodyPhysics>`;
//! that is the write path, and it is confined there.
//!
//! # Why this is not the engine's own components
//!
//! The de-adapter collapse (`docs/physics-deadapter-decision.md`) made authored
//! physics *be* avian's components, serialized directly, on the argument that a
//! mirror is framework cruft between the description of a body and the
//! operations on it. Three of that decision's four arguments still hold, and
//! nothing here reintroduces a per-frame translation or an engine-swap seam.
//!
//! The fourth does not survive: rapier's `Friction`, `Restitution`,
//! `ColliderMassProperties`, `GravityScale` and `LockedAxes` derive `Reflect`
//! but **not** `Serialize`, so engine components cannot be the save format.
//! Given that, owning the authored data outright is better than hand-writing
//! serde for someone else's types — and it is what lets `domain`, `command`,
//! `interaction` and `ui` stop naming the physics engine at all.
//!
//! It stays flat, `Copy`, and reflective, so records, undo capture and the
//! scripting registry all read it field-by-field exactly as before.

use serde::{Deserialize, Serialize};

/// Re-exported so consumers can name [`BodyPhysics::density`]'s type without
/// taking a dependency on the units crate — the same courtesy `domain` extends
/// for the shape tree.
pub use gradiance_units::Density;

/// How a body participates in the simulation.
///
/// The authored vocabulary, deliberately unchanged from what the editor has
/// always shown: a body is dynamic, immovable, or animated by hand.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub enum BodyKind {
    /// Simulated: forces, gravity and contacts move it.
    #[default]
    Dynamic,
    /// Immovable and infinitely massive — the ground, a pinned anchor.
    Static,
    /// Moved by the editor rather than the solver; pushes others, is not pushed.
    Kinematic,
}

impl BodyKind {
    /// Whether the solver integrates this body's motion.
    #[must_use]
    pub fn is_dynamic(self) -> bool {
        matches!(self, Self::Dynamic)
    }
}

/// Default friction for a new body.
#[must_use]
pub fn default_friction() -> f32 {
    0.5
}
/// Default restitution (bounciness) for a new body.
#[must_use]
pub fn default_restitution() -> f32 {
    0.3
}
/// Default areal mass density for a new body.
#[must_use]
pub fn default_density() -> Density {
    Density(1.0)
}
/// Default per-body gravity multiplier.
#[must_use]
pub fn default_gravity_scale() -> f32 {
    1.0
}

/// The authored physics of a body: the save content, the undo capture unit,
/// and the live component the physics layer syncs from.
#[derive(
    bevy::prelude::Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    bevy::reflect::Reflect,
)]
pub struct BodyPhysics {
    /// Simulation role.
    #[serde(default)]
    pub kind: BodyKind,
    /// Coulomb friction coefficient (dimensionless).
    #[serde(default = "default_friction")]
    pub friction: f32,
    /// Bounciness in `[0, 1]` (dimensionless).
    #[serde(default = "default_restitution")]
    pub restitution: f32,
    /// Areal mass density; mass is `density × area` (`units::mass_of`).
    #[serde(default = "default_density")]
    pub density: Density,
    /// Per-body gravity multiplier (dimensionless).
    #[serde(default = "default_gravity_scale")]
    pub gravity_scale: f32,
    /// Detects overlaps without colliding.
    #[serde(default)]
    pub sensor: bool,
    /// Prevents rotation in the simulation plane.
    ///
    /// Authored intent only. The engine's locked-axis set is *derived* from
    /// this together with the body's simulation-plane constraint, composed in
    /// one place in the physics layer — neither may clobber the other.
    #[serde(default)]
    pub rotation_locked: bool,
}

impl Default for BodyPhysics {
    fn default() -> Self {
        Self {
            kind: BodyKind::Dynamic,
            friction: default_friction(),
            restitution: default_restitution(),
            density: default_density(),
            gravity_scale: default_gravity_scale(),
            sensor: false,
            rotation_locked: false,
        }
    }
}

impl BodyPhysics {
    /// A fixed (non-simulating) body's physics — e.g. the ground. Everything
    /// but the role matches [`default`](BodyPhysics::default).
    #[must_use]
    pub fn fixed() -> Self {
        Self {
            kind: BodyKind::Static,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_editor_vocabulary() {
        let p = BodyPhysics::default();
        assert_eq!(p.kind, BodyKind::Dynamic);
        assert!(p.kind.is_dynamic());
        assert!(!p.sensor && !p.rotation_locked);
    }

    #[test]
    fn fixed_changes_only_the_role() {
        let (a, b) = (BodyPhysics::default(), BodyPhysics::fixed());
        assert_eq!(b.kind, BodyKind::Static);
        assert!(!b.kind.is_dynamic());
        assert_eq!(
            BodyPhysics {
                kind: BodyKind::Dynamic,
                ..b
            },
            a
        );
    }

    /// The save content must survive a round trip unchanged — this is the
    /// struct the scene format is made of.
    #[test]
    fn round_trips_through_ron() {
        let p = BodyPhysics {
            kind: BodyKind::Kinematic,
            friction: 0.25,
            restitution: 0.9,
            density: Density(2.5),
            gravity_scale: 0.0,
            sensor: true,
            rotation_locked: true,
        };
        let text = ron::ser::to_string(&p).expect("serializes");
        let back: BodyPhysics = ron::from_str(&text).expect("deserializes");
        assert_eq!(back, p);
    }

    /// Every field is optional on load, so a hand-written or partial scene
    /// still opens with sensible physics.
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let back: BodyPhysics = ron::from_str("()").expect("deserializes from empty");
        assert_eq!(back, BodyPhysics::default());
    }
}
