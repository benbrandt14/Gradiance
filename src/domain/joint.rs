//! Authored joint (constraint) definitions.
//!
//! A joint is its own authored entity: `StableId` + [`JointDef`] + the
//! [`Joint`](crate::domain::Joint) marker. The physics seam
//! (`physics::joint_sync`) derives the engine constraint from it; undo,
//! duplicate, delete, and persistence all treat joints exactly like
//! bodies (snapshot records), which is what keeps the combinatorics of
//! tools × commands × primitives tractable.
//!
//! # Adding a new constraint kind (the extension path)
//!
//! 1. Add a [`JointKind`] variant carrying its authored parameters
//!    (serde-compatible; reference bodies only by `StableId`).
//! 2. Add a match arm in `physics::joint_sync::derived_joint_bundle`
//!    mapping it onto the engine's native representation (avian joints
//!    are entities with components — prefer native limits/motors over
//!    hand-rolled controllers).
//! 3. Optionally add a tool (one file under `interaction/tools/`, gated
//!    on a new `ToolState`) and an inspector section (M7+). Existing
//!    commands (spawn/delete/duplicate/array/undo) work unchanged.
//!
//! Planned variants that slot in this way: `Spring { rest_length,
//! frequency, damping_ratio }` (avian `DistanceJoint`), `PlanarContact`,
//! `Cam { profile }`, `Magnet { strength, falloff }` (force field, not a
//! joint — pairs with a `physics/forces.rs` seam). Per-joint `breaking
//! force` and backlash are authored-parameter additions to
//! [`JointCommon`] with enforcement in the seam.

use crate::core::ids::StableId;
use bevy::math::Vec2;
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Motor settings shared by hinge (angular) and slider (linear) joints.
///
/// Maps onto the engine's native velocity-controlled motor with an
/// acceleration-based model (`stiffness = 0`, `damping` as configured).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct MotorDef {
    /// Target velocity (rad/s for hinges, px/s for sliders).
    pub target_velocity: f32,
    /// Maximum torque (hinge) or force (slider) the motor may exert.
    pub max_force: f32,
    /// Velocity-tracking damping of the motor model.
    pub damping: f32,
    /// Reverse direction at the joint limits (requires limits).
    pub oscillate: bool,
    /// Whether the motor is powered.
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

/// Parameters common to every joint kind.
#[derive(
    Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct JointCommon {
    /// Whether the two connected bodies still collide with each other
    /// (off by default, the Algodoo convention).
    pub collide_connected: bool,
}

/// The kind-specific parameters of a joint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub enum JointKind {
    /// Revolute: bodies rotate freely about the shared anchor.
    Hinge {
        /// Optional `[min, max]` relative-angle limits (radians).
        limits: Option<[f32; 2]>,
        /// Optional angular motor.
        motor: Option<MotorDef>,
    },
    /// Fixed: bodies are rigidly welded (rigid kinematic chains compose
    /// from welds).
    Weld,
    /// Prismatic: bodies slide along `axis` through the anchor.
    Slider {
        /// Slide axis in body-A local space (unit length).
        axis: Vec2,
        /// Optional `[min, max]` translation limits (pixels).
        limits: Option<[f32; 2]>,
        /// Optional linear motor.
        motor: Option<MotorDef>,
    },
}

/// The authored definition of one constraint between two bodies (or one
/// body and the world).
///
/// References bodies by [`StableId`], never `Entity`. `body_b == None`
/// pins `body_a` to the world at `anchor_b` (a **world-space** point);
/// otherwise `anchor_b` is body-B local.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct JointDef {
    /// Kind-specific parameters.
    pub kind: JointKind,
    /// Cross-kind parameters.
    pub common: JointCommon,
    /// First connected body.
    pub body_a: StableId,
    /// Second connected body, or `None` to pin to the world.
    pub body_b: Option<StableId>,
    /// Anchor in body-A local space (pixels).
    pub anchor_a: Vec2,
    /// Anchor in body-B local space — or world space for world pins.
    pub anchor_b: Vec2,
    /// Body A's rotation when the joint was authored (radians).
    ///
    /// Together with [`rest_rot_b`](Self::rest_rot_b) this defines the
    /// constraint's rest orientation: welds hold the bodies at their
    /// *creation-time* relative angle, sliders lock rotation to it, and
    /// hinge limits are measured from it. Without this, joints between
    /// rotated bodies violently snap into alignment at spawn.
    #[serde(default)]
    pub rest_rot_a: f32,
    /// Body B's rotation when the joint was authored (0 for world pins).
    #[serde(default)]
    pub rest_rot_b: f32,
}

impl JointDef {
    /// Ids of the bodies this joint references.
    pub fn referenced_bodies(&self) -> impl Iterator<Item = StableId> {
        std::iter::once(self.body_a).chain(self.body_b)
    }

    /// The joint's world-space anchor position, given body A's pose.
    ///
    /// This is where the glyph is drawn and where picking tests against —
    /// `anchor_a` is body-A local, so it rotates and translates with the
    /// body.
    pub fn anchor_world(&self, body_a_pos: Vec2, body_a_rot: f32) -> Vec2 {
        body_a_pos + Vec2::from_angle(body_a_rot).rotate(self.anchor_a)
    }
}
