//! Signal **actuators** on the physics side (brief N-1): the derived
//! [`SignalDrive`] override and the reads that prefer it over authored values.
//!
//! The signal layer evaluates a binding whose sink is an actuator (motor
//! speed, motor cap, field strength, gravity scale) and writes the result into
//! a [`SignalDrive`] on the target entity. This layer reads it where the
//! authored value would be read:
//!
//! - **motor speed / cap** — [`joint_sync::retune_joints`](crate::joint_sync::retune_joints)
//!   writes the driven value into the live engine joint; with no drive it
//!   restores the authored one ([`undriven_target`]);
//! - **field strength** — [`Fields`](crate::fields::Fields) samples the driven
//!   strength in place of `FieldSource::strength`;
//! - **gravity scale** — [`drive_gravity_scale`] adds avian's per-body
//!   [`ConstantLinearAcceleration`] for the difference between the driven
//!   and the authored scale (the authored `GravityScale` stays untouched);
//! - **thrust** — [`apply_thrusters`](crate::thrust::apply_thrusters) pushes
//!   with the driven thrust in place of the thruster node's authored one
//!   (brief N-3).
//!
//! Like `SignalColorOverride`, the drive is **derived**: never saved, never in
//! undo records, never read by commands (invariant 5). Unbinding removes it,
//! and the authored value comes back on the next frame with no command.

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;

/// The derived per-target actuator override: only the fields a binding
/// drives are `Some`. Lives on the joint, body or node entity its binding
/// names (by `StableId`); removed when no binding drives that target.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct SignalDrive {
    /// Motor target speed (rad/s for a hinge, m/s for a slider).
    pub motor_speed: Option<f32>,
    /// Motor torque or force cap (N·m or N), used as is; `<= 0` switches
    /// the motor off.
    pub motor_cap: Option<f32>,
    /// Field strength (m/s²; positive repels).
    pub field_strength: Option<f32>,
    /// Gravity scale (a ratio).
    pub gravity_scale: Option<f32>,
    /// A thruster node's thrust (N; brief N-3). Lives on the node entity.
    pub thrust: Option<f32>,
}

impl SignalDrive {
    /// Whether no field is driven (the component should go).
    pub const fn is_empty(&self) -> bool {
        self.motor_speed.is_none()
            && self.motor_cap.is_none()
            && self.field_strength.is_none()
            && self.gravity_scale.is_none()
            && self.thrust.is_none()
    }
}

/// The motor target an **undriven** joint should hold, given the authored
/// speed and the live one. A plain motor holds the authored speed. An
/// oscillating motor flips sign at its limits (`physics::motor`), so it keeps
/// the live direction and restores only the authored magnitude: a speed the
/// oscillator set is left alone, and one a removed drive left behind returns
/// to the authored magnitude. Pure.
#[must_use]
pub fn undriven_target(authored: f32, oscillate: bool, live: f32) -> f32 {
    if !oscillate || live == 0.0 {
        return authored;
    }
    if (live.abs() - authored.abs()).abs() <= f32::EPSILON * authored.abs().max(1.0) {
        live
    } else {
        authored.abs().copysign(live)
    }
}

/// The extra per-body acceleration that makes a body fall at `driven` times
/// gravity while avian applies the authored `authored` scale itself. Pure.
#[must_use]
pub fn gravity_correction(gravity: Vec2, authored: f32, driven: f32) -> Vec2 {
    gravity * (driven - authored)
}

/// Derives avian's [`ConstantLinearAcceleration`] from a driven gravity scale:
/// inserted while a drive sets the scale, kept equal to the difference from
/// the authored scale (gravity may change under it), removed when the drive
/// goes. The component is this system's alone: nothing authors it, nothing
/// saves it. Visits only driven bodies and bodies it left a correction on.
#[expect(clippy::type_complexity, reason = "one filtered query")]
pub fn drive_gravity_scale(
    mut commands: Commands,
    gravity: Res<Gravity>,
    mut bodies: Query<
        (
            Entity,
            Option<&SignalDrive>,
            Option<&GravityScale>,
            Option<&mut ConstantLinearAcceleration>,
        ),
        (
            With<Body>,
            Or<(With<SignalDrive>, With<ConstantLinearAcceleration>)>,
        ),
    >,
) {
    for (entity, drive, authored, current) in &mut bodies {
        let authored = authored.map_or(1.0, |g| g.0);
        let wanted = drive
            .and_then(|d| d.gravity_scale)
            .map(|driven| gravity_correction(gravity.0, authored, driven));
        match (wanted, current) {
            (Some(accel), Some(mut current)) => {
                if current.0 != accel {
                    current.0 = accel;
                }
            }
            (Some(accel), None) => {
                commands
                    .entity(entity)
                    .try_insert(ConstantLinearAcceleration(accel));
            }
            (None, Some(_)) => {
                commands
                    .entity(entity)
                    .try_remove::<ConstantLinearAcceleration>();
                // avian wakes a body when the component is *inserted or
                // changed* (its wake filters use `Changed`), not when it is
                // removed — so unbinding a gravity drive must wake the body
                // itself, or a body that slept while floated stays frozen
                // in mid-air with its gravity restored.
                commands.queue(WakeBody(entity));
            }
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undriven_plain_motor_holds_its_authored_speed() {
        assert!((undriven_target(2.0, false, -7.0) - 2.0).abs() < 1e-6);
        assert!((undriven_target(-2.0, false, 2.0) + 2.0).abs() < 1e-6);
    }

    #[test]
    fn an_undriven_oscillating_motor_keeps_its_direction() {
        // The oscillator flipped it: left alone.
        assert!((undriven_target(2.0, true, -2.0) + 2.0).abs() < 1e-6);
        // A drive left 5 behind going backwards: authored magnitude, same way.
        assert!((undriven_target(2.0, true, -5.0) + 2.0).abs() < 1e-6);
        // Stopped: start from the authored speed.
        assert!((undriven_target(2.0, true, 0.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_gravity_correction_tops_up_the_authored_scale() {
        let g = Vec2::new(0.0, -10.0);
        assert_eq!(gravity_correction(g, 1.0, 1.0), Vec2::ZERO);
        assert_eq!(gravity_correction(g, 1.0, 0.0), Vec2::new(0.0, 10.0));
        assert_eq!(gravity_correction(g, 0.5, -1.0), Vec2::new(0.0, 15.0));
    }

    #[test]
    fn an_empty_drive_says_so() {
        assert!(SignalDrive::default().is_empty());
        let drive = SignalDrive {
            gravity_scale: Some(0.0),
            ..default()
        };
        assert!(!drive.is_empty());
    }
}
