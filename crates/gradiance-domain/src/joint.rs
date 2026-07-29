//! Authored joint (constraint) definitions.
//!
//! A joint is its own authored entity: `StableId` + [`JointDef`] + the
//! [`Joint`](crate::Joint) marker. The physics seam
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

use bevy::math::Vec2;
use bevy::prelude::Component;
use gradiance_core::ids::StableId;
use serde::{Deserialize, Serialize};

// --- Motors ---------------------------------------------------------------
//
// A hinge motor and a slider motor are *different physical things*: one drives
// an angular velocity (rad/s) capped by a torque (N·m), the other a linear
// velocity (m/s) capped by a force (N). They used to share a single `MotorDef`
// whose `target_velocity` and `max_force` silently meant one or the other
// depending on the joint kind — the union behind the rad/s-vs-rpm and
// torque-vs-force mix-ups. Two types with typed quantities make that
// unrepresentable: the compiler now rejects a slider motor on a hinge, and the
// units come from `gradiance-units` so a label can't drift from its value.

/// The velocity-tracking gain (1/s) of the acceleration-based motor model: the
/// acceleration applied per unit of velocity error. It sets how firmly a motor
/// holds its target — too low and any load stalls it (the classic "weak
/// motor"). Instability in this model comes from high *stiffness*, not this
/// gain, so a firm value is safe.
pub const DEFAULT_MOTOR_DAMPING: f32 = 30.0;
/// Default hinge drive speed (rad/s ≈ 19 rpm).
pub const DEFAULT_MOTOR_ANGULAR_VELOCITY: f32 = 2.0;
/// Default slider drive speed (m/s).
pub const DEFAULT_MOTOR_LINEAR_VELOCITY: f32 = 2.0;

/// A **hinge** (revolute) motor: drives toward a target *angular* velocity,
/// capped by a maximum *torque*.
///
/// Maps onto avian's native velocity-controlled `AngularMotor` with an
/// acceleration-based model (`stiffness = 0`, `damping` as configured).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct AngularMotorDef {
    /// Target angular velocity (rad/s; the inspector shows it in rpm).
    pub target_velocity: gradiance_units::AngularVelocity,
    /// Maximum torque the motor may exert. `<= 0` means **auto**: the physics
    /// seam derives the ceiling from the driven body's real angular inertia
    /// (see [`motor_ceiling`]), so the motor holds firm across body sizes
    /// without the old fixed `1e7` — which sat above the solver's engagement
    /// impulse and spiked the rigid pivot off its pin.
    pub max_torque: gradiance_units::Torque,
    /// Velocity-tracking gain — see [`DEFAULT_MOTOR_DAMPING`].
    pub damping: f32,
    /// Reverse direction at the joint limits (requires limits).
    pub oscillate: bool,
    /// Whether the motor is powered.
    pub enabled: bool,
}

impl Default for AngularMotorDef {
    fn default() -> Self {
        Self {
            target_velocity: gradiance_units::AngularVelocity(DEFAULT_MOTOR_ANGULAR_VELOCITY),
            max_torque: gradiance_units::Torque(0.0), // auto
            damping: DEFAULT_MOTOR_DAMPING,
            oscillate: false,
            enabled: true,
        }
    }
}

/// A **slider** (prismatic) motor: drives toward a target *linear* velocity,
/// capped by a maximum *force*.
///
/// Maps onto avian's native velocity-controlled `LinearMotor`, same model as
/// [`AngularMotorDef`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct LinearMotorDef {
    /// Target linear velocity (m/s).
    pub target_velocity: gradiance_units::Velocity,
    /// Maximum force the motor may exert; `<= 0` = **auto** (scaled from the
    /// driven body's mass — see [`AngularMotorDef::max_torque`]).
    pub max_force: gradiance_units::Force,
    /// Velocity-tracking gain — see [`DEFAULT_MOTOR_DAMPING`].
    pub damping: f32,
    /// Reverse direction at the joint limits (requires limits).
    pub oscillate: bool,
    /// Whether the motor is powered.
    pub enabled: bool,
}

impl Default for LinearMotorDef {
    fn default() -> Self {
        Self {
            target_velocity: gradiance_units::Velocity(DEFAULT_MOTOR_LINEAR_VELOCITY),
            max_force: gradiance_units::Force(0.0), // auto
            damping: DEFAULT_MOTOR_DAMPING,
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
        /// Optional angular motor (rad/s driven, torque capped).
        motor: Option<AngularMotorDef>,
    },
    /// Prismatic: bodies slide along `axis` through the anchor.
    Slider {
        /// Slide axis in body-A local space (unit length).
        axis: Vec2,
        /// Optional `[min, max]` translation limits (m).
        limits: Option<[f32; 2]>,
        /// Optional linear motor (m/s driven, force capped).
        motor: Option<LinearMotorDef>,
    },
    /// Spring-damper strut: a soft distance constraint between the two
    /// anchors, drawn as a coil. Maps onto avian's `DistanceJoint`
    /// (`compliance` = `1 / stiffness`) plus a `JointDamping` component; when
    /// unbounded the distance limit pins to `rest_length` (a pure spring), and
    /// when a [`range`](Self::Spring::range) is set the limit becomes that band.
    Spring {
        /// The length the spring relaxes to (m) — the creation distance.
        rest_length: f32,
        /// Spring constant (stiffness, N/m); the joint's compliance is
        /// `1 / stiffness`, and `<= 0` is treated as rigid. Set from the
        /// connected mass at creation so the strut isn't too soft. The scalar a
        /// future curve editor would generalize to a nonlinear
        /// force-vs-displacement curve.
        stiffness: f32,
        /// Linear velocity damping applied by the joint (default 0). The scalar
        /// a future curve editor would generalize to a nonlinear damping curve.
        damping: f32,
        /// Optional hard length clamp `[min, max]` in metres; `None` (the
        /// default) leaves travel unbounded, so the spring is the only
        /// restoring force. When set, the strut floats freely within the band
        /// and springs back past either end.
        range: Option<[f32; 2]>,
    },
    /// Rigid link: the bodies hold their relative pose entirely.
    ///
    /// Distinct from the **weld tool**, which merges two bodies into one SDF
    /// union. Merging is the better answer when the result is genuinely one
    /// object — it cannot drift, because there is no constraint to drift. This
    /// is for when the two must stay *separate* bodies that happen to be held
    /// together: different materials, independently selectable and deletable,
    /// and each still its own `StableId`. A sketched link between two bodies is
    /// exactly that case, which is why the variant exists again after M20
    /// removed the old `Weld` (see `gradiance-scene`'s format history).
    ///
    /// It solves as a constraint, so it can drift under extreme load in a way
    /// a merge cannot. That is the price of keeping the bodies distinct.
    Fixed,
}

/// Fallback spring constant (N/m) for the inspector's reset (the strut tool
/// computes a mass-based value at creation; see
/// `interaction::tools::strut_tool`). Sized for a typical ~1 kg body: at
/// `k = m·g / sag` with `g ≈ 10` and a ~0.1 m sag this is ~100 N/m, matching
/// the tool's `SPRING_STIFFNESS_PER_MASS · mass`. (The earlier `0.1` was a
/// mechanical ÷`PIXELS_PER_METER²` rescale of the pixel-era value — three
/// orders too soft, so a reset strut drooped ~100 m.)
pub const DEFAULT_SPRING_STIFFNESS: f32 = 100.0;
/// Default linear damping for a freshly authored strut.
pub const DEFAULT_SPRING_DAMPING: f32 = 0.0;

// --- Mass-aware motor defaults -------------------------------------------
//
// A motor's `max_force` is a *torque/force ceiling*: avian clamps each
// substep's corrective impulse to `max_force · dt²`. The ceiling has a stable
// band, both ends of which the old fixed `1.0e7` fell outside of:
//
//   * Too LOW (below the load): the motor can't overcome gravity/contact and
//     reads as weak — a heavy body's need is `~m·g·r`, which exceeded the
//     fixed ceiling ("negligible torque").
//   * Too HIGH (above the acceleration model's engagement impulse, which is
//     `~ damping · target_velocity / dt · I` per unit inertia): the ceiling
//     never bites, so the first substep dumps a huge impulse into the joint
//     that the *rigid* point constraint can't absorb in one step — the pivot
//     visibly drifts ("the rotation point becomes offset, as if too
//     compliant"), and light bodies get flung.
//
// Scaling the ceiling with the connected body's **inertia** (angular) or
// **mass** (linear) keeps it inside that band across sizes: comfortably above
// the load, but below the spike threshold so the impulse stays bounded and
// the pivot holds. The coefficients are empirical (feel) — the one knob to
// nudge in-app — but sit ~1-2 orders under the spike threshold on purpose.

/// Default hinge-motor **max torque** per unit inertia proxy (N·m per kg·m²).
/// A 1 m unit-density box (inertia 1/6 kg·m²) gets ~330 N·m — dozens of times
/// its ~5 N·m gravity load, yet well under the drift-inducing spike ceiling.
pub const MOTOR_TORQUE_PER_INERTIA: f32 = 2.0e3;
/// Default slider-motor **max force** per unit mass proxy (N per kg). A 1 m
/// unit-density box (mass 1 kg) gets ~500 N vs its ~10 N weight.
pub const MOTOR_FORCE_PER_MASS: f32 = 5.0e2;
/// Nonzero floor so a tiny body still gets a usable motor ceiling.
pub const MIN_MOTOR_EFFORT: f32 = 1.0;

/// The motor's effective torque/force ceiling. An **explicit** authored
/// `max_force` (`> 0`) is used as-is; otherwise it is **auto** and scales with
/// the connected body's real `inertia_or_mass` (angular inertia for a hinge,
/// mass for a slider) via `per` (`MOTOR_TORQUE_PER_INERTIA` /
/// `MOTOR_FORCE_PER_MASS`), floored. The physics seam calls this at apply time
/// with the body's `Computed*` value, so every motor — authored in the UI,
/// spawned programmatically, or loaded from a scene — lands in the stable band.
#[must_use]
pub fn motor_ceiling(authored_max: f32, inertia_or_mass: f32, per: f32) -> f32 {
    if authored_max > 0.0 {
        authored_max
    } else {
        (per * inertia_or_mass).max(MIN_MOTOR_EFFORT)
    }
}

/// A hinge's relative angle **in avian's constraint frame** — the deviation of
/// body B from body A relative to the joint's creation pose, which is what
/// `with_angle_limits` (and the rest basis handed to `with_local_basis2`)
/// measures. Returns `0` at the creation pose, so the authored `[min, max]`
/// limits apply directly. Pure so the oscillate seam (`physics::motor`) stays
/// testable without a running solver.
#[must_use]
pub fn hinge_limit_angle(rot_a: f32, rot_b: f32, rest_rot_a: f32, rest_rot_b: f32) -> f32 {
    gradiance_geometry::wrap_angle((rot_b - rot_a) + (rest_rot_a - rest_rot_b))
}

/// The target velocity an oscillating motor should hold given its current
/// limit-frame angle `rel` (see [`hinge_limit_angle`]) — reverse toward the
/// interior once within `buffer` of either bound, otherwise keep driving
/// (`None`). `+velocity` drives `rel` upward, so the max bound reverses to
/// `-speed` and the min bound to `+speed`. Pure and unit-tested.
#[must_use]
pub fn oscillate_target(rel: f32, min: f32, max: f32, speed: f32, buffer: f32) -> Option<f32> {
    if rel >= max - buffer {
        Some(-speed.abs())
    } else if rel <= min + buffer {
        Some(speed.abs())
    } else {
        None
    }
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
    /// Anchor in body-A local space (m).
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

    #[test]
    fn motor_ceiling_auto_scales_with_body_and_respects_overrides() {
        // Auto (authored <= 0): scales with the connected body's inertia/mass.
        let small = motor_ceiling(0.0, 0.1, MOTOR_TORQUE_PER_INERTIA);
        let big = motor_ceiling(0.0, 5.0, MOTOR_TORQUE_PER_INERTIA);
        assert!(big > small, "heavier body -> higher ceiling");
        assert!((big - MOTOR_TORQUE_PER_INERTIA * 5.0).abs() < 1e-3);
        // A near-zero body still gets the usable floor, never zero/NaN.
        assert!(
            (motor_ceiling(0.0, 0.0, MOTOR_TORQUE_PER_INERTIA) - MIN_MOTOR_EFFORT).abs() < 1e-6
        );
        // An explicit authored cap wins over the auto scaling.
        assert!((motor_ceiling(42.0, 5.0, MOTOR_TORQUE_PER_INERTIA) - 42.0).abs() < 1e-6);
    }

    #[test]
    fn hinge_limit_angle_is_zero_at_creation() {
        // At the creation pose the limit-frame angle must be 0 so authored
        // [min, max] apply directly.
        assert!(hinge_limit_angle(0.7, 0.2, 0.7, 0.2).abs() < 1e-6);
        assert!(hinge_limit_angle(-1.3, 2.4, -1.3, 2.4).abs() < 1e-6);
        // A world pin (body B is the static anchor at rot 0, rest 0): the
        // angle is body A's deviation from its authored rest.
        let rest_a = 0.5;
        assert!((hinge_limit_angle(0.5 + 0.3, 0.0, rest_a, 0.0) - (-0.3)).abs() < 1e-6);
    }

    #[test]
    fn oscillate_reverses_at_each_bound() {
        let (min, max, speed, buf) = (-0.5, 0.5, 2.0, 0.05);
        // Interior: keep driving (no change).
        assert_eq!(oscillate_target(0.0, min, max, speed, buf), None);
        // Past the max bound: reverse to negative; past min: reverse to positive.
        assert_eq!(oscillate_target(0.48, min, max, speed, buf), Some(-2.0));
        assert_eq!(oscillate_target(-0.48, min, max, speed, buf), Some(2.0));
        // Sign of the input speed doesn't matter — direction comes from the bound.
        assert_eq!(oscillate_target(0.48, min, max, -2.0, buf), Some(-2.0));
    }

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
