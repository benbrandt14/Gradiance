//! The Ctrl-snap increment of a number field (brief U-8): pure, so the
//! rule is tested here and the UI only asks for it.
//!
//! Every value is in the field's **shown** unit ([`DisplayUnit`]): a field
//! showing centimetres snaps in centimetres. The rule, first match wins:
//!
//! 1. An **angle** snaps to 10° (shown in the field's unit, so a radian
//!    field snaps to 0.1745 rad).
//! 2. A field with a finite **range** snaps to a tenth of the range's
//!    decade (`0..1` → 0.1, `-180..180` → 10, `0..5` → 0.1).
//! 3. Otherwise the step is the decade of ten points of drag
//!    (`10 × speed`, rounded down to a power of ten): about one step per
//!    ten points the pointer moves. A field with a unit never snaps finer
//!    than one whole unit; a plain number (a ratio such as friction) may.
//!
//! [`fine`] is the Ctrl+Shift step, a tenth of it.

use crate::dimension::Dimension;
use crate::display::DisplayUnit;

/// The angle snap, in degrees.
pub const ANGLE_SNAP_DEG: f64 = 10.0;

/// The largest power of ten not above `x` (`1` for zero, negative, or not
/// finite).
#[must_use]
pub fn decade(x: f64) -> f64 {
    if x.is_finite() && x > 0.0 {
        10f64.powf(x.log10().floor())
    } else {
        1.0
    }
}

/// The Ctrl-snap step of a field showing `unit`, dragged at `speed` (shown
/// units per point), with an optional `range` (shown units). Always
/// positive and finite.
#[must_use]
pub fn snap_step(unit: DisplayUnit, speed: f64, range: Option<(f64, f64)>) -> f64 {
    if unit.dimension == Dimension::Angle {
        return unit.from_si(ANGLE_SNAP_DEG.to_radians());
    }
    if let Some((lo, hi)) = range {
        let span = hi - lo;
        if span.is_finite() && span > 0.0 {
            return decade(span) / 10.0;
        }
    }
    let by_speed = decade(10.0 * speed.abs());
    if unit.dimension == Dimension::Dimensionless {
        by_speed
    } else {
        by_speed.max(1.0)
    }
}

/// The Ctrl+Shift step: a tenth of `step`.
#[must_use]
pub fn fine(step: f64) -> f64 {
    step / 10.0
}

/// `value` rounded to the nearest multiple of `step` (unchanged when the
/// step is not a positive finite number).
#[must_use]
pub fn snap(value: f64, step: f64) -> f64 {
    if step.is_finite() && step > 0.0 {
        (value / step).round() * step
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{AngleUnit, LengthUnit, UnitPrefs};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn decades() {
        assert!(close(decade(1.0), 1.0));
        assert!(close(decade(9.99), 1.0));
        assert!(close(decade(10.0), 10.0));
        assert!(close(decade(0.05), 0.01));
        assert!(close(decade(360.0), 100.0));
        for bad in [0.0, -3.0, f64::NAN, f64::INFINITY] {
            assert!(close(decade(bad), 1.0), "{bad}");
        }
    }

    #[test]
    fn an_angle_snaps_to_ten_degrees_in_either_display() {
        let deg = UnitPrefs::default().display(Dimension::Angle);
        assert!(close(snap_step(deg, 1.0, None), 10.0));
        // The range does not override the angle rule.
        assert!(close(snap_step(deg, 1.0, Some((0.0, 1.0))), 10.0));
        let rad = UnitPrefs {
            angle: AngleUnit::Radians,
            ..UnitPrefs::default()
        }
        .display(Dimension::Angle);
        assert!(close(snap_step(rad, 0.01, None), 10f64.to_radians()));
        assert!(close(snap_step(DisplayUnit::DEGREES, 0.5, None), 10.0));
    }

    #[test]
    fn a_range_snaps_to_a_tenth_of_its_decade() {
        let ratio = DisplayUnit::canonical(Dimension::Dimensionless);
        assert!(close(snap_step(ratio, 0.01, Some((0.0, 1.0))), 0.1));
        assert!(close(snap_step(ratio, 0.01, Some((0.0, 5.0))), 0.1));
        assert!(close(snap_step(ratio, 0.01, Some((0.0, 1000.0))), 100.0));
        // An empty or unbounded range falls through to the speed rule.
        assert!(close(snap_step(ratio, 0.01, Some((1.0, 1.0))), 0.1));
        let unbounded = Some((f64::NEG_INFINITY, f64::INFINITY));
        assert!(close(snap_step(ratio, 0.01, unbounded), 0.1));
    }

    #[test]
    fn a_unit_field_snaps_to_whole_units_or_coarser() {
        let m = UnitPrefs::default().display(Dimension::Length);
        // 0.01 m per point: ten points is 0.1 m, so whole metres.
        assert!(close(snap_step(m, 0.01, None), 1.0));
        let cm = UnitPrefs {
            length: LengthUnit::Centimetres,
            ..UnitPrefs::default()
        }
        .display(Dimension::Length);
        // The same field in centimetres: 1 cm per point, 10 cm steps.
        assert!(close(snap_step(cm, 1.0, None), 10.0));
        let n = DisplayUnit::canonical(Dimension::Force);
        assert!(close(snap_step(n, 10.0, None), 100.0));
    }

    #[test]
    fn a_plain_number_snaps_by_its_speed() {
        let ratio = DisplayUnit::canonical(Dimension::Dimensionless);
        assert!(close(snap_step(ratio, 0.01, None), 0.1));
        assert!(close(snap_step(ratio, 0.05, None), 0.1));
        assert!(close(snap_step(ratio, 5.0, None), 10.0));
        assert!(close(snap_step(ratio, 0.0, None), 1.0));
    }

    #[test]
    fn snapping_rounds_to_the_nearest_step() {
        assert!(close(snap(0.34, 0.1), 0.3));
        assert!(close(snap(0.36, 0.1), 0.4));
        assert!(close(snap(-47.0, 10.0), -50.0));
        assert!(close(snap(3.3, 0.0), 3.3));
        assert!(close(snap(3.3, f64::NAN), 3.3));
        assert!(close(fine(10.0), 1.0));
    }
}
