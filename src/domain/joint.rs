//! Authored joint definitions.
//!
//! A joint is its own authored entity carrying a [`JointDef`] (+ `StableId`).
//! The physics seam resolves the referenced body ids and maintains a derived
//! engine joint alongside it.

use crate::core::ids::StableId;
use bevy::math::Vec2;
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Motor settings for hinge and slider joints.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotorDef {
    /// Target velocity (rad/s for hinges, px/s for sliders).
    pub target_velocity: f32,
    /// Maximum torque (hinge) or force (slider) the motor may exert.
    pub max_force: f32,
    /// Velocity-tracking damping (acceleration-based motor model).
    pub damping: f32,
    /// When set, the motor reverses direction at the joint limits.
    pub oscillate: bool,
    /// Whether the motor is currently powered.
    pub enabled: bool,
}

impl Default for MotorDef {
    fn default() -> Self {
        Self {
            target_velocity: 2.0,
            max_force: 1.0e7,
            damping: 1.0,
            oscillate: false,
            enabled: true,
        }
    }
}

/// The kind-specific parameters of a joint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JointKind {
    /// Revolute joint: bodies rotate freely around the shared anchor.
    Hinge {
        /// Optional `[min, max]` relative-angle limits in radians.
        limits: Option<[f32; 2]>,
        /// Optional angular motor.
        motor: Option<MotorDef>,
    },
    /// Fixed joint: bodies are rigidly welded at the anchor.
    Weld,
    /// Prismatic joint: bodies slide along `axis` through the anchor.
    Slider {
        /// Slide axis in body-A local space (unit length).
        axis: Vec2,
        /// Optional `[min, max]` translation limits in pixels.
        limits: Option<[f32; 2]>,
        /// Optional linear motor.
        motor: Option<MotorDef>,
    },
}

/// The authored definition of a joint between two bodies.
///
/// References bodies by [`StableId`], never by `Entity`. `body_b == None`
/// pins `body_a` to the world at the anchor.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JointDef {
    /// Kind-specific parameters.
    pub kind: JointKind,
    /// First connected body.
    pub body_a: StableId,
    /// Second connected body, or `None` to pin to the world.
    pub body_b: Option<StableId>,
    /// Anchor in body-A local space (pixels).
    pub anchor_a: Vec2,
    /// Anchor in body-B local space, or world space for world pins.
    pub anchor_b: Vec2,
}
