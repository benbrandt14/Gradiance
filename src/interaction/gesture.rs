//! Gesture constraints: axis locks and rotation quantization.
//!
//! CAD-style modifiers, updated every frame from the keyboard and consumed
//! by tools during drag gestures (and by future gesture behaviors):
//!
//! | Modifier | Effect |
//! |---|---|
//! | hold `X` | horizontal-only translation |
//! | hold `Y` | vertical-only translation |
//! | hold `Shift` | dominant-axis lock (larger delta component wins) |
//! | hold `Ctrl` while rotating | quantize to `SnapConfig::rotation_step_deg` |

use crate::domain::settings::SnapConfig;
use crate::geometry::snapping::{constrain_axis, quantize_angle};
use bevy::prelude::*;

/// Active translation constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AxisConstraint {
    /// Unconstrained.
    #[default]
    Free,
    /// X-only.
    Horizontal,
    /// Y-only.
    Vertical,
    /// Whichever axis dominates the delta.
    Dominant,
}

/// The current gesture modifiers, as a resource so every tool (and any
/// future gesture behavior) shares one source of truth.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct GestureConstraints {
    /// Translation constraint.
    pub axis: AxisConstraint,
    /// Quantize rotation gestures to the configured step.
    pub quantize_rotation: bool,
}

impl GestureConstraints {
    /// Applies the axis constraint to a translation delta.
    pub fn constrain(&self, delta: Vec2) -> Vec2 {
        match self.axis {
            AxisConstraint::Free => delta,
            AxisConstraint::Horizontal => constrain_axis(delta, true, false, false),
            AxisConstraint::Vertical => constrain_axis(delta, false, true, false),
            AxisConstraint::Dominant => constrain_axis(delta, false, false, true),
        }
    }

    /// Applies rotation quantization (radians) when active.
    pub fn apply_rotation(&self, angle: f32, config: &SnapConfig) -> f32 {
        if self.quantize_rotation {
            quantize_angle(angle, config.rotation_step_deg.to_radians())
        } else {
            angle
        }
    }
}

/// Reads modifier keys into [`GestureConstraints`].
pub fn update_gesture_constraints(
    keys: Res<ButtonInput<KeyCode>>,
    mut constraints: ResMut<GestureConstraints>,
) {
    constraints.axis = if keys.pressed(KeyCode::KeyX) {
        AxisConstraint::Horizontal
    } else if keys.pressed(KeyCode::KeyY) {
        AxisConstraint::Vertical
    } else if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        AxisConstraint::Dominant
    } else {
        AxisConstraint::Free
    };
    constraints.quantize_rotation =
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
}
