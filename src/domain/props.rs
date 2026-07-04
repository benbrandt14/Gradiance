//! Authored physical properties of a body.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// How a body participates in the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BodyKind {
    /// Fully simulated: forces, gravity, collisions.
    #[default]
    Dynamic,
    /// Immovable (ground, anchors).
    Static,
    /// Moved only by direct writes; pushes dynamic bodies.
    Kinematic,
}

/// The authored physical properties of a body.
///
/// The physics seam translates these into engine components on change.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicalProps {
    /// Simulation role.
    pub body: BodyKind,
    /// Mass density (collider area × density = mass).
    pub density: f32,
    /// Coulomb friction coefficient.
    pub friction: f32,
    /// Bounciness in `[0, 1]`.
    pub restitution: f32,
    /// Per-body gravity multiplier.
    pub gravity_scale: f32,
    /// Detects overlaps without colliding.
    pub sensor: bool,
    /// Prevents all rotation when set.
    pub rotation_locked: bool,
}

impl Default for PhysicalProps {
    fn default() -> Self {
        Self {
            body: BodyKind::Dynamic,
            density: 1.0,
            friction: 0.5,
            restitution: 0.3,
            gravity_scale: 1.0,
            sensor: false,
            rotation_locked: false,
        }
    }
}
