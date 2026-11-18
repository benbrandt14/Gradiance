//! Parameter tuning: the defaults and edit ranges of the physical parameters
//! a user edits, chosen so that a default behaves visibly on any body from a
//! pebble to a truck and wild behaviour sits only at the ends of a range
//! (brief 4b-4; the measurements are in its Outcome).
//!
//! Two kinds of parameter live here:
//!
//! - **Mass-relative** ones are authored in units that do not mention mass,
//!   and the physics seam derives the engine's number from the bodies' current
//!   mass at sync time. A spring is authored as a natural frequency and a
//!   damping ratio ([`spring_stiffness`], [`spring_damping_rate`]), so it keeps
//!   its feel when a body's size or density changes.
//! - **Dimensionless** ones (restitution, friction) need no scaling; their
//!   range only keeps an edit inside the band the solver handles.
//!
//! Everything here is pure, so the scaling is unit-tested without a solver;
//! the behaviour over real bodies is pinned by `tests/it/defaults.rs`.

use std::f32::consts::TAU;

/// The edit range of one parameter: the band a field clamps to and the
/// default a fresh value takes. A **log** range spreads its positions
/// evenly over ratios (a frequency), a linear one over differences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditRange {
    /// Lowest value an edit commits.
    pub min: f32,
    /// Highest value an edit commits.
    pub max: f32,
    /// The value a fresh parameter takes (and a field's reset).
    pub default: f32,
    /// Positions are spaced by ratio rather than by difference.
    pub log: bool,
}

impl EditRange {
    /// Clamps `value` into the range (a NaN becomes the default).
    #[must_use]
    pub fn clamp(&self, value: f32) -> f32 {
        if value.is_nan() {
            self.default
        } else {
            value.clamp(self.min, self.max)
        }
    }

    /// The value at slider position `t` (0 = `min`, 1 = `max`; clamped).
    #[must_use]
    pub fn at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if self.log {
            (self.min.ln() + t * (self.max.ln() - self.min.ln())).exp()
        } else {
            self.min + t * (self.max - self.min)
        }
    }

    /// The slider position of `value` (inverse of [`at`](Self::at)).
    #[must_use]
    pub fn position(&self, value: f32) -> f32 {
        let v = self.clamp(value);
        let t = if self.log {
            (v.ln() - self.min.ln()) / (self.max.ln() - self.min.ln())
        } else {
            (v - self.min) / (self.max - self.min)
        };
        t.clamp(0.0, 1.0)
    }
}

/// A spring's natural frequency (Hz). The default bounces about 1.5 times a
/// second, the sag of the old `k = 100·m` default (about 0.1 m under 1 g),
/// whatever the body weighs. Below 0.1 Hz a spring is too soft to hold a
/// body up (a 0.1 Hz spring sags 25 m); above about 20 Hz it is
/// indistinguishable from a rod.
pub const SPRING_FREQUENCY: EditRange = EditRange {
    min: 0.1,
    max: 20.0,
    default: 1.5,
    log: true,
};

/// A spring's damping ratio (0 = undamped, 1 = critical: no overshoot). The
/// default bounces visibly a few times and settles within about 3 s.
pub const SPRING_DAMPING_RATIO: EditRange = EditRange {
    min: 0.0,
    max: 2.0,
    default: 0.1,
    log: false,
};

/// Restitution: 0 = no bounce, 1 = the solver's most elastic contact (a box
/// keeps about 77% of its drop height). Above 1 a contact adds energy, so the
/// range stops at 1.
pub const RESTITUTION: EditRange = EditRange {
    min: 0.0,
    max: 1.0,
    default: 0.3,
    log: false,
};

/// Friction coefficient: 0 = ice; 1 holds a box on a 45° slope; 2 is
/// rubber on rubber. The default 0.5 holds on a 25° slope and slides on 30°.
pub const FRICTION: EditRange = EditRange {
    min: 0.0,
    max: 2.0,
    default: 0.5,
    log: false,
};

/// The reduced mass of a two-body constraint from the bodies' **inverse**
/// masses (a static, kinematic or absent side has inverse mass 0). `0` when
/// neither side moves. The same reduction gives the effective angular inertia
/// from inverse inertias.
#[must_use]
pub fn reduced(inv_a: f32, inv_b: f32) -> f32 {
    let w = inv_a.max(0.0) + inv_b.max(0.0);
    if w > 0.0 { 1.0 / w } else { 0.0 }
}

/// The inverse of a body's mass (or inertia) for [`reduced`]: `0` for a body
/// that does not move or whose mass is not known yet.
#[must_use]
pub fn inverse(value: f32, moves: bool) -> f32 {
    if moves && value > 0.0 && value.is_finite() {
        1.0 / value
    } else {
        0.0
    }
}

/// The spring constant (N/m) that gives natural frequency `frequency_hz` on
/// a constraint of reduced mass `effective_mass`: `k = m·(2πf)²`. The period
/// is then independent of the mass, which is the point. `0` when nothing
/// moves (the sync treats a non-positive stiffness as rigid, and a rigid
/// strut between two fixed bodies is still at rest).
#[must_use]
pub fn spring_stiffness(frequency_hz: f32, effective_mass: f32) -> f32 {
    let omega = TAU * frequency_hz.max(0.0);
    effective_mass.max(0.0) * omega * omega
}

/// The relative-velocity damping rate (1/s) avian's `JointDamping::linear`
/// takes for damping ratio `ratio` at `frequency_hz`: `2ζω`. avian applies it
/// to the reduced mass itself, so it carries no mass.
#[must_use]
pub fn spring_damping_rate(frequency_hz: f32, ratio: f32) -> f32 {
    2.0 * ratio.max(0.0) * TAU * frequency_hz.max(0.0)
}

/// avian's per-substep motor gain for a velocity-tracking `rate` (1/s).
///
/// avian 0.8's acceleration-based motor changes the relative velocity by
/// `gain · error` **per substep**, so its `damping` is not a rate: a gain
/// above about 2 overshoots by more than the error every substep and the
/// motor stalls or spins away (measured: the old default 30 left a wheel
/// pinned at its centre still, and spun a 10 cm one at 400 rad/s). The rate
/// is converted with the substep length and capped at 1, the dead-beat gain,
/// so any authored rate is stable.
#[must_use]
pub fn motor_gain(rate: f32, substep_secs: f32) -> f32 {
    (rate.max(0.0) * substep_secs.max(0.0)).min(1.0)
}

/// Default break force per unit of the connected **weight**: a breakable
/// joint holds ten times the weight it carries. A pendulum swinging from
/// horizontal pulls three times its weight, so a default joint survives
/// ordinary motion and breaks under a deliberate load.
pub const BREAK_FORCE_PER_WEIGHT: f32 = 10.0;
/// Floor for [`default_break_force`] (N), for a weightless assembly.
pub const MIN_BREAK_FORCE: f32 = 0.01;

/// The break force a joint gets when "breakable" is switched on: ten times
/// the weight (`mass · gravity`) of the bodies it connects that move.
#[must_use]
pub fn default_break_force(moving_mass: f32, gravity: f32) -> f32 {
    (BREAK_FORCE_PER_WEIGHT * moving_mass.max(0.0) * gravity.abs()).max(MIN_BREAK_FORCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, rel: f32) -> bool {
        (a - b).abs() <= rel * a.abs().max(b.abs()).max(1e-6)
    }

    #[test]
    fn ranges_hold_their_defaults_and_round_trip_positions() {
        for r in [
            SPRING_FREQUENCY,
            SPRING_DAMPING_RATIO,
            RESTITUTION,
            FRICTION,
        ] {
            assert!(r.min < r.default && r.default <= r.max, "{r:?}");
            assert!(close(r.at(0.0), r.min, 1e-5) && close(r.at(1.0), r.max, 1e-5));
            for t in [0.1, 0.5, 0.9] {
                assert!(close(r.position(r.at(t)), t, 1e-4), "{r:?} at {t}");
            }
            assert!((r.clamp(f32::NAN) - r.default).abs() < f32::EPSILON);
            assert!((r.clamp(r.max * 10.0 + 1.0) - r.max).abs() < f32::EPSILON);
        }
        // A log range spaces positions by ratio: its midpoint is the
        // geometric mean.
        let mid = SPRING_FREQUENCY.at(0.5);
        assert!(close(
            mid,
            (SPRING_FREQUENCY.min * SPRING_FREQUENCY.max).sqrt(),
            1e-4
        ));
    }

    #[test]
    fn the_body_defaults_sit_in_their_ranges() {
        use crate::props::{default_friction, default_restitution};
        assert!((default_restitution().coefficient - RESTITUTION.default).abs() < 1e-6);
        assert!((default_friction().dynamic_coefficient - FRICTION.default).abs() < 1e-6);
    }

    #[test]
    fn reduced_mass_ignores_fixed_sides() {
        assert!(close(
            reduced(inverse(2.0, true), inverse(2.0, true)),
            1.0,
            1e-6
        ));
        assert!(close(
            reduced(inverse(5.0, true), inverse(1e9, false)),
            5.0,
            1e-6
        ));
        assert!(reduced(inverse(5.0, false), inverse(0.0, true)).abs() < f32::EPSILON);
        assert!(inverse(f32::INFINITY, true).abs() < f32::EPSILON);
    }

    #[test]
    fn a_spring_keeps_its_period_at_any_mass() {
        // Period = 2π·sqrt(m/k) = 1/f for every mass from a pebble to a truck.
        for m in [0.01_f32, 0.1, 1.0, 100.0, 1000.0] {
            let k = spring_stiffness(1.5, m);
            let period = TAU * (m / k).sqrt();
            assert!(close(period, 1.0 / 1.5, 1e-4), "m = {m}: period {period}");
        }
        // Stiffer with frequency (monotonic), zero when nothing moves.
        assert!(spring_stiffness(2.0, 1.0) > spring_stiffness(1.0, 1.0));
        assert!(spring_stiffness(1.5, 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_damping_rate_is_twice_omega() {
        let omega = TAU * 1.5;
        assert!(close(spring_damping_rate(1.5, 1.0), 2.0 * omega, 1e-6));
        assert!(spring_damping_rate(1.5, 0.0).abs() < f32::EPSILON);
        assert!(
            spring_damping_rate(1.5, -1.0).abs() < f32::EPSILON,
            "negative ratio clamps"
        );
    }

    #[test]
    fn motor_gain_is_capped_at_dead_beat() {
        let dt = 1.0 / 360.0;
        assert!(close(motor_gain(36.0, dt), 0.1, 1e-5));
        assert!((motor_gain(1.0e6, dt) - 1.0).abs() < f32::EPSILON);
        assert!(motor_gain(-5.0, dt).abs() < f32::EPSILON);
    }

    #[test]
    fn break_force_scales_with_the_carried_weight() {
        let pebble = default_break_force(0.01, 9.81);
        let truck = default_break_force(1000.0, 9.81);
        assert!(close(truck / pebble, 1.0e5, 1e-4));
        assert!(close(pebble, 10.0 * 0.01 * 9.81, 1e-5));
        assert!((default_break_force(0.0, 9.81) - MIN_BREAK_FORCE).abs() < f32::EPSILON);
    }
}
