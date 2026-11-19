//! Numbers in the units a script author thinks in (brief 5-4).
//!
//! A script number is a plain `f64` in base SI: every verb reads and writes
//! metres, radians, kilograms and seconds. These functions are the
//! conversions the script verbs `q`, `convert` and `unit-of` call at the
//! authoring boundary. A unit here is anything [`evaluate`] reads (`cm`,
//! `km/h`, `N m`, `deg/s`, even `25 cm`), so there is one grammar for a typed
//! field and a script, and nothing about units reaches the numeric kernel.
//!
//! A conversion between two units checks that they measure the same thing:
//! `convert(90.0, "deg", "m")` is an error, not 1.57. That check is how far
//! a scripted value carries its dimension; a number that has left a
//! conversion is plain again.

use crate::Dimension;
use crate::expr::{Dims, ExprError, Value, evaluate};

/// `value` of `unit`, in base SI.
///
/// ```
/// use gradiance_units::convert::to_si;
/// assert!((to_si(25.0, "cm").unwrap() - 0.25).abs() < 1e-12);
/// assert!((to_si(36.0, "km/h").unwrap() - 10.0).abs() < 1e-12);
/// assert!(to_si(1.0, "furlong").is_err());
/// ```
///
/// # Errors
/// [`ExprError`] when `unit` does not evaluate, or the result is not finite.
pub fn to_si(value: f64, unit: &str) -> Result<f64, ExprError> {
    finite(value * evaluate(unit)?.si)
}

/// `value` of `from`, expressed in `to`. Both are units of one dimension.
///
/// ```
/// use gradiance_units::convert::convert;
/// assert!((convert(0.25, "m", "cm").unwrap() - 25.0).abs() < 1e-9);
/// assert!((convert(1.0, "rev", "deg").unwrap() - 360.0).abs() < 1e-9);
/// assert!(convert(90.0, "deg", "m").is_err());
/// ```
///
/// # Errors
/// [`ExprError`] when either unit does not evaluate,
/// [`ExprError::WrongDimension`] when they measure different things.
pub fn convert(value: f64, from: &str, to: &str) -> Result<f64, ExprError> {
    convert_values(value, evaluate(from)?, evaluate(to)?)
}

/// `value` of the unit `from`, expressed in the unit `to`, both already
/// evaluated (a caller may build `to` from a [`Dimension`] with
/// [`si_unit`]).
///
/// # Errors
/// [`ExprError::WrongDimension`] when the dimensions differ,
/// [`ExprError::NotFinite`] for a zero-sized `to`.
pub fn convert_values(value: f64, from: Value, to: Value) -> Result<f64, ExprError> {
    if from.dims != to.dims {
        return Err(ExprError::WrongDimension {
            expected: to.dims.to_string(),
            found: from.dims.to_string(),
        });
    }
    finite(value * from.si / to.si)
}

/// One base-SI unit of `dimension` (`1 m`, `1 m/s`) as a [`Value`].
#[must_use]
pub const fn si_unit(dimension: Dimension) -> Value {
    Value {
        si: 1.0,
        dims: dimension.dims(),
    }
}

/// The SI symbol of what `unit` measures: `km/h` reads `m/s`, `kN` reads
/// `N`, and a pure number reads `""` (as a count's unit does). An exponent
/// list no [`Dimension`] names reads as one (`m^3`).
///
/// Torque and energy share exponents (N·m = J), so a unit that is itself a
/// canonical symbol answers itself: `J` reads `J`, `N·m` reads `N·m`, and
/// `kJ` reads the first of the two, `N·m`.
///
/// ```
/// use gradiance_units::convert::si_symbol;
/// assert_eq!(si_symbol("km/h").unwrap(), "m/s");
/// assert_eq!(si_symbol("J").unwrap(), "J");
/// assert_eq!(si_symbol("pi").unwrap(), "");
/// ```
///
/// # Errors
/// [`ExprError`] when `unit` does not evaluate.
pub fn si_symbol(unit: &str) -> Result<String, ExprError> {
    let dims = evaluate(unit)?.dims;
    let named = Dimension::ALL
        .into_iter()
        .find(|d| d.dims() == dims && d.symbol() == unit.trim());
    Ok(match named {
        Some(d) => d.symbol().to_owned(),
        None if dims == Dims::NONE => String::new(),
        None => dims.to_string(),
    })
}

/// Every unit name the parser knows, space-separated: what an error lists.
#[must_use]
pub fn known_units() -> String {
    crate::expr::UNITS
        .iter()
        .map(|u| u.symbol)
        .collect::<Vec<_>>()
        .join(" ")
}

fn finite(si: f64) -> Result<f64, ExprError> {
    if si.is_finite() {
        Ok(si)
    } else {
        Err(ExprError::NotFinite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::UNITS;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    /// Every unit converts to SI and back to itself, and `to_si` agrees
    /// with the field parser reading the same text (one parser).
    #[test]
    fn every_unit_round_trips_through_the_field_parser() {
        for u in UNITS {
            for value in [0.0, 1.0, -2.5, 25.0, 1e-3, 360.0] {
                let si = to_si(value, u.symbol).unwrap_or_else(|e| panic!("{}: {e}", u.symbol));
                let typed = evaluate(&format!("{value} {}", u.symbol))
                    .unwrap_or_else(|e| panic!("{}: {e}", u.symbol));
                assert!(
                    close(si, typed.si),
                    "{value} {}: {si} vs {}",
                    u.symbol,
                    typed.si
                );
                let back = convert_values(si, si_unit_of(u.dims), evaluate(u.symbol).unwrap())
                    .unwrap_or_else(|e| panic!("{}: {e}", u.symbol));
                assert!(close(back, value), "{value} {} came back {back}", u.symbol);
            }
        }
    }

    fn si_unit_of(dims: Dims) -> Value {
        Value { si: 1.0, dims }
    }

    #[test]
    fn conversions_between_units() {
        assert!(close(convert(25.0, "cm", "m").unwrap(), 0.25));
        assert!(close(convert(0.25, "m", "cm").unwrap(), 25.0));
        assert!(close(
            convert(90.0, "deg", "rad").unwrap(),
            core::f64::consts::FRAC_PI_2
        ));
        assert!(close(
            convert(60.0, "rpm", "rad/s").unwrap(),
            core::f64::consts::TAU
        ));
        assert!(close(convert(1.0, "kN", "N").unwrap(), 1000.0));
        assert!(close(convert(5.0, "J", "N m").unwrap(), 5.0));
        assert!(close(convert(3.0, "ft", "in").unwrap(), 36.0));
        assert!(close(convert(1.0, "m", "m").unwrap(), 1.0));
    }

    #[test]
    fn a_dimension_mismatch_is_an_error_that_names_both() {
        let e = convert(90.0, "deg", "m").unwrap_err();
        assert_eq!(
            e,
            ExprError::WrongDimension {
                expected: "m".into(),
                found: "rad".into()
            }
        );
        assert_eq!(e.to_string(), "expected m, got rad");
        assert!(
            convert(1.0, "Hz", "rad/s").is_err(),
            "angle is its own base"
        );
        assert!(convert(1.0, "m", "pi").is_err());
    }

    #[test]
    fn a_bad_unit_is_an_error() {
        assert_eq!(
            to_si(1.0, "furlong"),
            Err(ExprError::UnknownUnit("furlong".into()))
        );
        assert_eq!(to_si(1.0, ""), Err(ExprError::Empty));
        assert!(to_si(1.0, "cm cm +").is_err());
        assert!(convert(1.0, "m", "0 m").is_err(), "a zero-sized unit");
        assert!(to_si(f64::INFINITY, "m").is_err());
    }

    #[test]
    fn si_symbols() {
        for d in Dimension::ALL {
            let canonical = si_unit(d);
            assert_eq!(canonical.dims, d.dims());
            if !d.symbol().is_empty() {
                assert_eq!(si_symbol(d.symbol()).unwrap(), d.symbol(), "{d:?}");
            }
        }
        assert_eq!(si_symbol("km/h").unwrap(), "m/s");
        assert_eq!(si_symbol("25 cm").unwrap(), "m");
        assert_eq!(si_symbol("deg/s").unwrap(), "rad/s");
        assert_eq!(si_symbol("kJ").unwrap(), "N·m");
        assert_eq!(si_symbol("m^3").unwrap(), "m^3");
        assert_eq!(si_symbol("2 pi").unwrap(), "");
        assert!(si_symbol("parsec").is_err());
    }

    #[test]
    fn known_units_lists_the_table() {
        let list = known_units();
        for u in UNITS {
            assert!(list.split(' ').any(|s| s == u.symbol), "{}", u.symbol);
        }
    }
}
