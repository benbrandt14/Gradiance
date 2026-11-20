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
//! breaking force landed as an authored-parameter addition to [`JointCommon`]
//! ([`JointCommon::break_force`]) with enforcement in the seam; backlash is
//! planned the same way.

use bevy::math::Vec2;
use bevy::prelude::Component;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_units::{Angle, Displacement, Force, Frequency, Length};
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

/// The velocity-tracking rate (1/s) of the acceleration-based motor model: the
/// fraction of the velocity error a motor removes per second. It sets how
/// firmly a motor holds its target; too low and any load slows it (the
/// classic "weak motor"). avian takes a per-substep gain, so the physics seam
/// converts this with the substep length and caps it at the dead-beat gain
/// ([`crate::tuning::motor_gain`]): any rate is stable. The default is that
/// cap at the default 360 substeps/s (brief 4b-4 measured the old 30,
/// passed to avian unconverted, as a gain of 30: a centre-pinned wheel did
/// not turn).
pub const DEFAULT_MOTOR_DAMPING: f32 = 360.0;
/// Default hinge drive speed (rad/s): exactly 20 rpm, the unit the
/// inspector shows, so a fresh motor reads `20 rpm` rather than the
/// `19.099 rpm` that 2 rad/s gave (brief A-2).
pub const DEFAULT_MOTOR_ANGULAR_VELOCITY: f32 = 20.0 * std::f32::consts::TAU / 60.0;
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
///
/// The two breaking fields are skipped when at their defaults, so a scene
/// without breakable joints serializes exactly as before they existed (no
/// format bump; the replay goldens stay byte-identical).
#[derive(
    Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub struct JointCommon {
    /// Whether the two connected bodies still collide with each other
    /// (off by default, the Algodoo convention).
    pub collide_connected: bool,
    /// The **linear** reaction force above which the joint breaks during a
    /// run; `None` (the default) = unbreakable. Compared against the
    /// magnitude of avian's post-solve `JointForces::force` by
    /// `physics::joint_sync::break_overloaded_joints` — see [`exceeds_break_force`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_force: Option<Force>,
    /// Whether the joint has broken. **Authored** state, written by the
    /// physics seam during a run exactly as the solver writes poses, so the
    /// run boundary records it (undo restores the joint) and a save keeps it
    /// broken. The live avian `JointDisabled` is *derived* from it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub broken: bool,
}

/// The break force (N) a joint gets when "breakable" is switched on and the
/// weight it carries is not known: ten times a 1 kg body's weight. The
/// inspector scales it to the connected bodies instead
/// ([`crate::tuning::default_break_force`]).
pub const DEFAULT_BREAK_FORCE: f32 = 100.0;

/// Whether a joint's reaction-force magnitude `force` breaks a joint with
/// threshold `break_force`: strictly above it, and never for an unbreakable
/// (`None`) joint. A non-positive threshold breaks on any load. Pure.
#[must_use]
pub fn exceeds_break_force(break_force: Option<Force>, force: Force) -> bool {
    break_force.is_some_and(|limit| force.value() > limit.value())
}

/// The load fraction at which a joint glyph reads fully **hot**: a joint at
/// 90 % of the force that breaks it is about to fail (brief N-9).
pub const HOT_LOAD: f32 = 0.9;

/// How loaded a joint is, as a fraction of the force it is rated for: its
/// `break_force` when it can break, else the break force "breakable" would
/// give it ([`crate::tuning::default_break_force`]: ten times the weight of
/// the moving bodies it connects, `moving_mass` kg under `gravity` m/s²). So
/// an unbreakable joint reads how close it would be to failing, and a
/// hanging pendulum reads a tenth, not 1. Never negative. Pure.
#[must_use]
pub fn load_fraction(
    force: Force,
    break_force: Option<Force>,
    moving_mass: f32,
    gravity: f32,
) -> f32 {
    let rated = break_force.map_or_else(
        || crate::tuning::default_break_force(moving_mass, gravity),
        Force::value,
    );
    if rated <= 0.0 {
        // A non-positive threshold breaks on any load (`exceeds_break_force`).
        return if force.value() > 0.0 { 1.0 } else { 0.0 };
    }
    (force.value() / rated).max(0.0)
}

/// The kind-specific parameters of a joint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub enum JointKind {
    /// Revolute: bodies rotate freely about the shared anchor.
    Hinge {
        /// Optional `[min, max]` relative-angle limits.
        limits: Option<[Angle; 2]>,
        /// Optional angular motor (rad/s driven, torque capped).
        motor: Option<AngularMotorDef>,
    },
    /// Prismatic: bodies slide along `axis` through the anchor.
    Slider {
        /// Slide axis in body-A local space (unit length).
        axis: Vec2,
        /// Optional `[min, max]` translation limits.
        limits: Option<[Length; 2]>,
        /// Optional linear motor (m/s driven, force capped).
        motor: Option<LinearMotorDef>,
    },
    /// Spring-damper strut: a soft distance constraint between the two
    /// anchors, drawn as a coil. Maps onto avian's `DistanceJoint`
    /// (`compliance` = `1 / stiffness`) plus a `JointDamping` component; when
    /// unbounded the distance limit pins to `rest_length` (a pure spring), and
    /// when a [`range`](Self::Spring::range) is set the limit becomes that band.
    ///
    /// Authored **mass-relative** (brief 4b-4): a natural frequency and a
    /// damping ratio. The physics seam derives the stiffness (N/m) and the
    /// damping rate from the bodies' current reduced mass
    /// ([`crate::tuning::spring_stiffness`]), and re-derives them when a mass
    /// changes, so a spring keeps its period when a body is resized or made
    /// denser. The derived values are never saved.
    Spring {
        /// The length the spring relaxes to — the creation distance.
        rest_length: Length,
        /// Natural frequency of the bounce (Hz); `<= 0` is treated as rigid.
        /// See [`crate::tuning::SPRING_FREQUENCY`].
        frequency: Frequency,
        /// Damping ratio: 0 = undamped, 1 = critical (no overshoot). See
        /// [`crate::tuning::SPRING_DAMPING_RATIO`].
        damping_ratio: f32,
        /// Optional hard length clamp `[min, max]`; `None` (the
        /// default) leaves travel unbounded, so the spring is the only
        /// restoring force. When set, the strut floats freely within the band
        /// and springs back past either end.
        range: Option<[Length; 2]>,
    },
}

/// Default natural frequency of a freshly authored spring (Hz).
pub const DEFAULT_SPRING_FREQUENCY: f32 = crate::tuning::SPRING_FREQUENCY.default;
/// Default damping ratio of a freshly authored spring.
pub const DEFAULT_SPRING_DAMPING_RATIO: f32 = crate::tuning::SPRING_DAMPING_RATIO.default;

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
/// the joint's **reduced** `inertia_or_mass` (angular inertia for a hinge, mass
/// for a slider; a static side counts as immovable, so a wheel on a static
/// ground is sized by the wheel, not the ground) via `per`
/// (`MOTOR_TORQUE_PER_INERTIA` / `MOTOR_FORCE_PER_MASS`), floored. The physics
/// seam (`physics::joint_sync::retune_joints`) re-applies it whenever a mass
/// changes, so every motor — authored in the UI, spawned programmatically, or
/// loaded from a scene — lands in the stable band.
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

/// The pose a world pin's "body B" sits at — the identity. See
/// [`JointDef::sensor_state`].
pub const WORLD_POSE: PosRot = PosRot {
    pos: Vec2::ZERO,
    rot: 0.0,
};

/// A joint's live sensor readings ([`JointDef::sensor_state`]) — typed SI
/// quantities (`docs/units-decision.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointSensors {
    /// Distance between the two world anchors.
    pub length: Length,
    /// Relative rotation minus the authored rest angle (hinge-limit frame).
    pub angle: Angle,
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
    /// Anchor in body-A local space.
    pub anchor_a: Displacement,
    /// Anchor in body-B local space — or world space for world pins.
    pub anchor_b: Displacement,
    /// Body A's rotation when the joint was authored.
    ///
    /// Together with [`rest_rot_b`](Self::rest_rot_b) this defines the
    /// constraint's rest orientation: welds hold the bodies at their
    /// *creation-time* relative angle, sliders lock rotation to it, and
    /// hinge limits are measured from it. Without this, joints between
    /// rotated bodies violently snap into alignment at spawn.
    #[serde(default)]
    pub rest_rot_a: Angle,
    /// Body B's rotation when the joint was authored (0 for world pins).
    #[serde(default)]
    pub rest_rot_b: Angle,
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
        body_a_pos + Vec2::from_angle(body_a_rot).rotate(self.anchor_a.value())
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
            JointKind::Hinge { .. } if self.body_b.is_none() => self.rest_rot_a.value(),
            _ => body_a_rot,
        }
    }

    /// Whether the derived engine joint puts the **world** first: a hinge or
    /// slider pinned to the world is derived as `(pin, body A)`, so that a
    /// positive motor speed, the limits and the angle or travel all describe
    /// the body moving against the world, as body B moves against body A on
    /// a body-to-body joint: a hinge turns it counter-clockwise, a slider
    /// drives it along the drawn axis (briefs A-1, A-2; before, the pin moved
    /// against the body, so a world-pin motor ran backwards and its limits
    /// were mirrored). A spring is symmetric and keeps its order.
    #[must_use]
    pub fn world_first(&self) -> bool {
        self.body_b.is_none()
            && matches!(
                self.kind,
                JointKind::Hinge { .. } | JointKind::Slider { .. }
            )
    }

    /// A **slider**'s travel frame in world space: the origin its `[min, max]`
    /// limits are measured from and the unit axis direction, given body A's
    /// world anchor and live rotation; `None` for other kinds. The gizmo, the
    /// limit handles and the oscillate seam all read it, so they agree with
    /// the engine joint.
    ///
    /// Body-to-body, the travel starts at body A's anchor and turns with
    /// body A (body B slides along it). On a world pin
    /// ([`world_first`](Self::world_first)) the travel is fixed in the world:
    /// it starts at the pin and points along the axis at body A's authored
    /// rest angle, and body A's anchor slides along it.
    #[must_use]
    pub fn slider_frame(&self, anchor_a_world: Vec2, rot_a: f32) -> Option<(Vec2, Vec2)> {
        let JointKind::Slider { axis, .. } = self.kind else {
            return None;
        };
        Some(if self.world_first() {
            (
                self.anchor_b.value(),
                Vec2::from_angle(self.rest_rot_a.value()).rotate(axis),
            )
        } else {
            (anchor_a_world, Vec2::from_angle(rot_a).rotate(axis))
        })
    }

    /// A **slider**'s live travel — the position avian's limits and motor act
    /// on — from both bodies' poses ([`WORLD_POSE`] for a world pin's
    /// `pose_b`): the slid anchor's offset from the
    /// [`slider_frame`](Self::slider_frame) origin along its axis. 0 at
    /// creation; a positive motor speed drives it up. `None` for other kinds.
    #[must_use]
    pub fn slider_travel(&self, pose_a: PosRot, pose_b: PosRot) -> Option<f32> {
        let world_a = self.anchor_world(pose_a.pos, pose_a.rot);
        let (origin, dir) = self.slider_frame(world_a, pose_a.rot)?;
        let slid = if self.world_first() {
            world_a
        } else {
            pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(self.anchor_b.value())
        };
        Some((slid - origin).dot(dir))
    }

    /// The hinge angle avian's limits and motor act on, from both bodies'
    /// live rotations (`rot_b` = 0 for a world pin): the driven body against
    /// the reference body, minus the authored rest angle — body B against A,
    /// or for a world pin ([`world_first`](Self::world_first)) body A against
    /// the world. 0 at creation; a positive motor speed drives it up.
    #[must_use]
    pub fn hinge_angle(&self, rot_a: f32, rot_b: f32) -> f32 {
        let (rest_a, rest_b) = (self.rest_rot_a.value(), self.rest_rot_b.value());
        if self.world_first() {
            hinge_limit_angle(rot_b, rot_a, rest_b, rest_a)
        } else {
            hinge_limit_angle(rot_a, rot_b, rest_a, rest_b)
        }
    }

    /// The joint's live **sensor state** given both endpoint poses — the pure
    /// math behind the `JointLength`/`JointAngle` signal sources (read through
    /// `physics::queries`). Derived, never saved.
    ///
    /// - `length`: distance between the two **world** anchors (`anchor_a` on
    ///   body A, `anchor_b` on body B).
    /// - `angle`: [`hinge_angle`](Self::hinge_angle), wrapped to `(-π, π]` —
    ///   the frame avian's hinge limits and the oscillating motor measure in,
    ///   so a plotted angle compares directly to the authored `[min, max]`.
    ///   For a world pin it is body A against the world.
    ///
    /// A **world pin** (`body_b == None`) passes [`WORLD_POSE`] for `pose_b`:
    /// its `anchor_b` is already a world point and the derived static pin sits
    /// at rotation 0, so the world *is* body B at the identity pose.
    #[must_use]
    pub fn sensor_state(&self, pose_a: PosRot, pose_b: PosRot) -> JointSensors {
        let world_a = self.anchor_world(pose_a.pos, pose_a.rot);
        let world_b = pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(self.anchor_b.value());
        JointSensors {
            length: Length(world_a.distance(world_b)),
            angle: Angle(self.hinge_angle(pose_a.rot, pose_b.rot)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_is_read_against_the_break_force_or_the_default_rating() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // Breakable: against its own threshold.
        assert!(close(
            load_fraction(Force(90.0), Some(Force(100.0)), 1.0, 9.81),
            0.9
        ));
        // Unbreakable: against ten times the carried weight, so a body
        // hanging still reads a tenth.
        assert!(close(load_fraction(Force(9.81), None, 1.0, 9.81), 0.1));
        // A zero threshold is hot under any load, calm under none.
        assert!(close(load_fraction(Force(1.0), Some(Force(0.0)), 1.0, 9.81), 1.0));
        assert!(close(load_fraction(Force(0.0), Some(Force(0.0)), 1.0, 9.81), 0.0));
    }

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
        // A world pin (the static anchor at rot 0, rest 0) puts the world
        // first: the angle is body A's deviation from its authored rest.
        let def = hinge(None, 0.5);
        assert!(def.world_first());
        assert!((def.hinge_angle(0.5 + 0.3, 0.0) - 0.3).abs() < 1e-6);
        assert!(!hinge(Some(StableId::new()), 0.5).world_first());
    }

    fn pose(x: f32, y: f32, rot: f32) -> PosRot {
        PosRot {
            pos: Vec2::new(x, y),
            rot,
        }
    }

    #[test]
    fn a_hinge_at_rest_reads_zero_angle_and_zero_length() {
        // Two bodies 2 m apart, hinged at the midpoint, both authored rotated.
        let mut def = hinge(Some(StableId::new()), 0.4);
        def.rest_rot_b = Angle(-0.2);
        def.anchor_a = Displacement(Vec2::new(1.0, 0.0));
        def.anchor_b = Displacement(Vec2::new(-1.0, 0.0));
        // Body-local anchors in each body's rotated frame: place the bodies so
        // both anchors land on the same world point.
        let pivot = Vec2::new(3.0, 1.0);
        let pa = pivot - Vec2::from_angle(0.4).rotate(def.anchor_a.value());
        let pb = pivot - Vec2::from_angle(-0.2).rotate(def.anchor_b.value());
        let s = def.sensor_state(pose(pa.x, pa.y, 0.4), pose(pb.x, pb.y, -0.2));
        assert!(s.length.value() < 1e-5, "coincident anchors: {s:?}");
        assert!(s.angle.value().abs() < 1e-6, "at rest: {s:?}");
        // Rotate B by +0.3 about the pivot: angle reads +0.3, length stays 0.
        let pb2 = pivot - Vec2::from_angle(0.1).rotate(def.anchor_b.value());
        let s2 = def.sensor_state(pose(pa.x, pa.y, 0.4), pose(pb2.x, pb2.y, 0.1));
        assert!(close(s2.angle.value(), 0.3), "{s2:?}");
        assert!(s2.length.value() < 1e-5);
    }

    #[test]
    fn a_slider_reads_its_extension_as_length() {
        let def = JointDef {
            kind: JointKind::Slider {
                axis: Vec2::X,
                limits: None,
                motor: None,
            },
            common: JointCommon::default(),
            body_a: StableId::new(),
            body_b: Some(StableId::new()),
            anchor_a: Displacement(Vec2::ZERO),
            anchor_b: Displacement(Vec2::ZERO),
            rest_rot_a: Angle(0.0),
            rest_rot_b: Angle(0.0),
        };
        let s = def.sensor_state(pose(0.0, 0.0, 0.0), pose(1.5, 0.0, 0.0));
        assert!(close(s.length.value(), 1.5), "{s:?}");
        assert!(s.angle.value().abs() < 1e-6);
    }

    #[test]
    fn a_world_pin_measures_against_the_world_point() {
        // Pinned at world (2, 0); body A's anchor is 1 m right of its centre.
        let mut def = hinge(None, 0.5);
        def.anchor_a = Displacement(Vec2::new(1.0, 0.0));
        def.anchor_b = Displacement(Vec2::new(2.0, 0.0));
        // At rest (rot 0.5) with the anchor 0.5 m short of the pin.
        let a = Vec2::new(1.5, 0.0) - Vec2::from_angle(0.5).rotate(def.anchor_a.value());
        let s = def.sensor_state(pose(a.x, a.y, 0.5), WORLD_POSE);
        assert!(close(s.length.value(), 0.5), "{s:?}");
        assert!(s.angle.value().abs() < 1e-6);
        // Body A swings +0.3 (counter-clockwise): the angle is the body
        // against the world, so it reads +0.3, the sign the gizmo's limit
        // arc (`rest_rot_a + [min, max]`) and a positive motor use.
        let s2 = def.sensor_state(pose(a.x, a.y, 0.8), WORLD_POSE);
        assert!(close(s2.angle.value(), 0.3), "{s2:?}");
    }

    #[test]
    fn only_a_breakable_joint_over_its_threshold_breaks() {
        assert!(
            !exceeds_break_force(None, Force(1.0e9)),
            "None never breaks"
        );
        assert!(!exceeds_break_force(Some(Force(10.0)), Force(10.0)));
        assert!(exceeds_break_force(Some(Force(10.0)), Force(10.5)));
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
                limits: Some([Angle(-0.5), Angle(0.5)]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: StableId::new(),
            body_b,
            anchor_a: Displacement(Vec2::ZERO),
            anchor_b: Displacement(Vec2::ZERO),
            rest_rot_a: Angle(rest_rot_a),
            rest_rot_b: Angle(0.0),
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
                limits: Some([Length(-1.0), Length(1.0)]),
                motor: None,
            },
            common: JointCommon::default(),
            body_a: StableId::new(),
            body_b: None,
            anchor_a: Displacement(Vec2::ZERO),
            anchor_b: Displacement(Vec2::ZERO),
            rest_rot_a: Angle(0.7),
            rest_rot_b: Angle(0.0),
        };
        assert!(close(def.limit_reference_angle(1.3), 1.3));
    }

    /// A world-pin slider's travel is fixed in the world at the pin, along
    /// the axis at body A's rest angle, and reads body A's offset from the
    /// pin; a body-to-body slider's starts at body A's anchor and reads body
    /// B's offset (brief A-2).
    #[test]
    fn a_world_pin_slider_measures_the_body_from_the_pin() {
        let rest = std::f32::consts::FRAC_PI_2;
        let mut def = hinge(None, rest);
        def.kind = JointKind::Slider {
            axis: Vec2::X,
            limits: None,
            motor: None,
        };
        def.anchor_b = Displacement(Vec2::new(1.0, 2.0));
        assert!(def.world_first());
        // The body has slid 0.5 m up its (world +Y) axis.
        let pose_a = PosRot {
            pos: Vec2::new(1.0, 2.5),
            rot: rest,
        };
        let anchor = def.anchor_world(pose_a.pos, pose_a.rot);
        let (origin, dir) = def.slider_frame(anchor, pose_a.rot).unwrap();
        assert!(origin.distance(Vec2::new(1.0, 2.0)) < 1e-6);
        assert!(dir.distance(Vec2::Y) < 1e-6);
        assert!(close(def.slider_travel(pose_a, WORLD_POSE).unwrap(), 0.5));

        // Body-to-body: B sits 0.5 m along A's (rotated) axis.
        def.body_b = Some(StableId::new());
        def.anchor_b = Displacement(Vec2::ZERO);
        let pose_a = PosRot {
            pos: Vec2::ZERO,
            rot: rest,
        };
        let pose_b = PosRot {
            pos: Vec2::new(0.0, 0.5),
            rot: 0.0,
        };
        assert!(!def.world_first());
        assert!(close(def.slider_travel(pose_a, pose_b).unwrap(), 0.5));
        assert!(hinge(None, 0.0).slider_travel(pose_a, pose_b).is_none());
    }
}
