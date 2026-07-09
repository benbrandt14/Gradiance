//! The authored physics of a body — avian's own components.
//!
//! The de-adapter collapse (`docs/physics-deadapter-decision.md`) removed the
//! engine-agnostic `PhysicalProps` mirror and its per-frame translation. A
//! body's live authored physics *is* the avian components on its entity
//! (`RigidBody`, `Friction`, `Restitution`, `ColliderDensity`, `GravityScale`,
//! and optional `Sensor` / `LockedAxes`); operations, queries, and scripts read
//! and edit them directly. [`BodyPhysics`] is the grouped value object used only
//! for capture, persistence, and undo — never a live component.

use avian2d::prelude::*;
use bevy::ecs::world::{EntityRef, EntityWorldMut};
use serde::{Deserialize, Serialize};

/// Default friction for a new body (matches the pre-collapse 0.5).
pub fn default_friction() -> Friction {
    Friction::new(0.5)
}
/// Default restitution for a new body (0.3).
pub fn default_restitution() -> Restitution {
    Restitution::new(0.3)
}
/// Default collider density for a new body (1.0).
pub fn default_density() -> ColliderDensity {
    ColliderDensity(1.0)
}
/// Default per-body gravity multiplier (1.0).
pub fn default_gravity_scale() -> GravityScale {
    GravityScale(1.0)
}

/// The authored physics of a body, grouped for capture / persistence / undo.
///
/// Not a live component: the entity carries these avian components directly.
/// This value object assembles them ([`capture`](BodyPhysics::capture)) and
/// applies them ([`insert_into`](BodyPhysics::insert_into)), so records and the
/// scene file stay one flat struct.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BodyPhysics {
    /// Simulation role.
    #[serde(default)]
    pub rigid_body: RigidBody,
    /// Coulomb friction.
    #[serde(default = "default_friction")]
    pub friction: Friction,
    /// Bounciness in `[0, 1]`.
    #[serde(default = "default_restitution")]
    pub restitution: Restitution,
    /// Mass density (collider area × density = mass).
    #[serde(default = "default_density")]
    pub density: ColliderDensity,
    /// Per-body gravity multiplier.
    #[serde(default = "default_gravity_scale")]
    pub gravity_scale: GravityScale,
    /// Detects overlaps without colliding.
    #[serde(default)]
    pub sensor: bool,
    /// Prevents all rotation when set.
    #[serde(default)]
    pub rotation_locked: bool,
}

impl Default for BodyPhysics {
    fn default() -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
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
    /// Assembles the authored physics from an entity's avian components,
    /// falling back to defaults for any that are absent.
    pub fn capture(entity: &EntityRef) -> Self {
        Self {
            rigid_body: entity.get::<RigidBody>().copied().unwrap_or_default(),
            friction: entity
                .get::<Friction>()
                .copied()
                .unwrap_or_else(default_friction),
            restitution: entity
                .get::<Restitution>()
                .copied()
                .unwrap_or_else(default_restitution),
            density: entity
                .get::<ColliderDensity>()
                .copied()
                .unwrap_or_else(default_density),
            gravity_scale: entity
                .get::<GravityScale>()
                .copied()
                .unwrap_or_else(default_gravity_scale),
            sensor: entity.contains::<Sensor>(),
            rotation_locked: entity.contains::<LockedAxes>(),
        }
    }

    /// Inserts the authored physics onto an entity (marker presence handled).
    pub fn insert_into(&self, entity: &mut EntityWorldMut) {
        entity.insert((
            self.rigid_body,
            self.friction,
            self.restitution,
            self.density,
            self.gravity_scale,
        ));
        if self.sensor {
            entity.insert(Sensor);
        } else {
            entity.remove::<Sensor>();
        }
        if self.rotation_locked {
            entity.insert(LockedAxes::ROTATION_LOCKED);
        } else {
            entity.remove::<LockedAxes>();
        }
    }
}
