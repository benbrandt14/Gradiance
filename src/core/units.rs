//! Small strongly-typed value types shared across layers.

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

/// A 2D position + rotation pair — the authored transform of a body.
///
/// This is the unit moved by transform commands and stored in snapshots;
/// it deliberately excludes scale (bodies are resized by editing their
/// [`ShapeDef`](crate::domain::shape::ShapeDef), never by scaling).
///
/// Capture from a Bevy `Transform` round-trips its X/Y and Z-rotation
/// while dropping Z-depth and scale — and it quantizes rotation so that
/// `save → load → save` reaches a byte-stable fixpoint:
///
/// ```
/// use gradiance::core::units::PosRot;
/// use bevy::prelude::Transform;
/// use bevy::math::Vec3;
///
/// let mut t = Transform::from_translation(Vec3::new(10.0, -4.0, 99.0));
/// t.rotation = bevy::math::Quat::from_rotation_z(0.5);
///
/// let pose = PosRot::from_transform(&t);
/// assert_eq!(pose.pos, bevy::math::Vec2::new(10.0, -4.0)); // Z dropped
/// assert!((pose.rot - 0.5).abs() < 1e-4);
///
/// // Writing back preserves the Transform's Z and scale.
/// let mut back = Transform::from_translation(Vec3::new(0.0, 0.0, 99.0));
/// pose.apply_to(&mut back);
/// assert_eq!(back.translation.z, 99.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PosRot {
    /// World-space translation in pixels.
    pub pos: Vec2,
    /// Rotation around +Z in radians.
    pub rot: f32,
}

impl PosRot {
    /// Rotation capture resolution in radians (≈ 0.0006°).
    ///
    /// Quaternion→angle extraction is not bit-idempotent (it oscillates by
    /// ~1 ULP), which would make save→load→save never reach a byte-stable
    /// fixpoint. Snapping to a grid ~40× coarser than that noise — and far
    /// below any physical significance — makes capture deterministic.
    const ROT_RESOLUTION: f32 = 1e-5;

    /// Builds a [`PosRot`] from a Bevy [`Transform`](bevy::prelude::Transform),
    /// discarding Z and scale.
    pub fn from_transform(transform: &bevy::prelude::Transform) -> Self {
        let raw = transform.rotation.to_euler(bevy::math::EulerRot::ZYX).0;
        Self {
            pos: transform.translation.truncate(),
            rot: (raw / Self::ROT_RESOLUTION).round() * Self::ROT_RESOLUTION,
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
