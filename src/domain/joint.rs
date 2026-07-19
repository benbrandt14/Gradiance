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
//! [`JointKind::Spring`] (a spring-damper strut over avian's `DistanceJoint`)
//! landed this way. Further planned variants that slot in the same way:
//! `PlanarContact`, `Cam { profile }`, `Magnet { strength, falloff }` (force
//! field, not a joint — pairs with a `physics/forces.rs` seam). Per-joint
//! `breaking force` and backlash are authored-parameter additions to
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
    /// Velocity-tracking gain of the acceleration-based motor model: the
    /// angular/linear acceleration applied per unit of velocity error
    /// (units 1/s). It sets how firmly the motor holds its target velocity —
    /// too low and any load stalls it (the classic "weak motor"). Instability
    /// in this model comes from high *stiffness*, not this gain, so a firm
    /// value is safe.
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
            // A firm velocity gain: ~2-frame time constant at 60 fps, so the
            // motor actually holds its target under load instead of drifting
            // (the earlier default of 1.0 read as "motors are very weak").
            damping: 30.0,
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
    /// Prismatic: bodies slide along `axis` through the anchor.
    Slider {
        /// Slide axis in body-A local space (unit length).
        axis: Vec2,
        /// Optional `[min, max]` translation limits (pixels).
        limits: Option<[f32; 2]>,
        /// Optional linear motor.
        motor: Option<MotorDef>,
    },
    /// Spring-damper strut: a soft distance constraint between the two
    /// anchors, drawn as a coil. Maps onto avian's `DistanceJoint`
    /// (`compliance` = `1 / stiffness`) plus a `JointDamping` component; when
    /// unbounded the distance limit pins to `rest_length` (a pure spring), and
    /// when a [`range`](Self::Spring::range) is set the limit becomes that band.
    Spring {
        /// The length the spring relaxes to (px) — the creation distance.
        rest_length: f32,
        /// Spring constant (stiffness, N/px); the joint's compliance is
        /// `1 / stiffness`, and `<= 0` is treated as rigid. Set from the
        /// connected mass at creation so the strut isn't too soft. The scalar a
        /// future curve editor would generalize to a nonlinear
        /// force-vs-displacement curve.
        stiffness: f32,
        /// Linear velocity damping applied by the joint (default 0). The scalar
        /// a future curve editor would generalize to a nonlinear damping curve.
        damping: f32,
        /// Optional hard length clamp `[min, max]` in pixels; `None` (the
        /// default) leaves travel unbounded, so the spring is the only
        /// restoring force. When set, the strut floats freely within the band
        /// and springs back past either end.
        range: Option<[f32; 2]>,
    },
}

/// Fallback spring constant for the inspector's reset (the strut tool computes
/// a mass-based value at creation; see `interaction::tools::strut_tool`).
pub const DEFAULT_SPRING_STIFFNESS: f32 = 1000.0;
/// Default linear damping for a freshly authored strut.
pub const DEFAULT_SPRING_DAMPING: f32 = 0.0;

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

    /// The world angle a **hinge** limit arc is measured from — shared by the
    /// gizmo, the pick test, and the limit-handle drag so all three agree.
    ///
    /// For a **world pin** (`body_b == None`) the constraint is against the
    /// fixed pin frame, which sits at body A's authored `rest_rot_a`; the
    /// allowed range is `rest_rot_a + [min, max]` in *world* angles, so the arc
    /// must anchor to `rest_rot_a` and **not** rotate with the swinging body
    /// (the old bug). For a body-to-body hinge, body A *is* the reference frame,
    /// so the arc tracks A's live rotation. (Sliders always use the live angle;
    /// only a world-pin hinge substitutes the rest frame.)
    pub fn limit_reference_angle(&self, body_a_rot: f32) -> f32 {
        match self.kind {
            JointKind::Hinge { .. } if self.body_b.is_none() => self.rest_rot_a,
            _ => body_a_rot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hinge(body_b: Option<StableId>, rest_rot_a: f32) -> JointDef {
        JointDef {
            kind: JointKind::Hinge {
                limits: Some([-0.5, 0.5]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: StableId::new(),
            body_b,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            rest_rot_a,
            rest_rot_b: 0.0,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn a_world_pin_hinge_anchors_its_limit_arc_to_the_rest_frame() {
        // World pin: the arc stays at rest_rot_a regardless of the body's live
        // rotation — it must not rotate with the swinging body.
        let def = hinge(None, 0.7);
        assert!(close(def.limit_reference_angle(1.3), 0.7));
    }

    #[test]
    fn a_body_to_body_hinge_tracks_body_a() {
        // Body A is the reference frame, so the arc follows its live rotation.
        let def = hinge(Some(StableId::new()), 0.7);
        assert!(close(def.limit_reference_angle(1.3), 1.3));
    }

    #[test]
    fn a_world_pin_slider_keeps_the_live_angle() {
        // Only world-pin hinges substitute the rest frame; sliders don't.
        let def = JointDef {
            kind: JointKind::Slider {
                axis: Vec2::X,
                limits: Some([-1.0, 1.0]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: StableId::new(),
            body_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            rest_rot_a: 0.7,
            rest_rot_b: 0.0,
        };
        assert!(close(def.limit_reference_angle(1.3), 1.3));
    }
}
