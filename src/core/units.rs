//! Small strongly-typed value types shared across layers.

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

/// A 2D position + rotation pair — the authored transform of a body.
///
/// This is the unit moved by transform commands and stored in snapshots;
/// it deliberately excludes scale (bodies are resized by editing their
/// [`ShapeDef`](crate::domain::shape::ShapeDef), never by scaling).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PosRot {
    /// World-space translation in pixels.
    pub pos: Vec2,
    /// Rotation around +Z in radians.
    pub rot: f32,
}

impl PosRot {
    /// Builds a [`PosRot`] from a Bevy [`Transform`](bevy::prelude::Transform),
    /// discarding Z and scale.
    pub fn from_transform(transform: &bevy::prelude::Transform) -> Self {
        Self {
            pos: transform.translation.truncate(),
            rot: transform.rotation.to_euler(bevy::math::EulerRot::ZYX).0,
        }
    }

    /// Writes this pose onto a Bevy [`Transform`](bevy::prelude::Transform),
    /// preserving its Z translation and scale.
    pub fn apply_to(&self, transform: &mut bevy::prelude::Transform) {
        transform.translation.x = self.pos.x;
        transform.translation.y = self.pos.y;
        transform.rotation = bevy::math::Quat::from_rotation_z(self.rot);
    }
}
