//! Display units: which unit a field shows a quantity in, and the
//! preference that picks it (brief 5-3).
//!
//! Display only: authored values, the save format, scripts and the
//! operation surface stay base SI. A [`DisplayUnit`] converts at the field:
//! it formats an SI value in the shown unit and parses a typed entry back
//! to SI ([`DisplayUnit::parse`]), where a bare number means the shown
//! unit and a number with a unit converts (`25 cm` in a metre field is
//! `0.25`).

use crate::Dimension;
use crate::expr::{ExprError, evaluate};
use bevy::reflect::Reflect;
use serde::{Deserialize, Serialize};

/// How angles show: degrees or radians.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
pub enum AngleUnit {
    /// Degrees (`°`, `°/s`). The default: the joint inspector's limits
    /// were already shown in degrees.
    #[default]
    Degrees,
    /// Radians (`rad`, `rad/s`), as stored.
    Radians,
}

impl AngleUnit {
    /// Every choice, in menu order (the first is the default).
    pub const ALL: [Self; 2] = [Self::Degrees, Self::Radians];

    /// The stable name operations use (`deg`, `rad`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Degrees => "deg",
            Self::Radians => "rad",
        }
    }

    /// The choice called `name`, if any.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.name() == name)
    }
}

/// How lengths show: metres, centimetres or millimetres.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
pub enum LengthUnit {
    /// Metres, as stored.
    #[default]
    Metres,
    /// Centimetres.
    Centimetres,
    /// Millimetres.
    Millimetres,
}

impl LengthUnit {
    /// Every choice, in menu order (the first is the default).
    pub const ALL: [Self; 3] = [Self::Metres, Self::Centimetres, Self::Millimetres];

    /// The stable name operations use, which is also the symbol shown.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Metres => "m",
            Self::Centimetres => "cm",
            Self::Millimetres => "mm",
        }
    }

    /// The choice called `name`, if any.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.name() == name)
    }

    /// Shown units per metre.
    const fn per_metre(self) -> f64 {
        match self {
            Self::Metres => 1.0,
            Self::Centimetres => 100.0,
            Self::Millimetres => 1000.0,
        }
    }
}

/// The display preference: one choice per dimension that has one. Angles
/// cover angular velocity too (`°/s` or `rad/s`); the length scale covers
/// lengths only (a velocity stays `m/s`). Every other dimension shows its
/// canonical SI unit.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[serde(default)]
pub struct UnitPrefs {
    /// How angles show.
    pub angle: AngleUnit,
    /// How lengths show.
    pub length: LengthUnit,
}

impl UnitPrefs {
    /// The unit a field of dimension `d` shows under this preference.
    ///
    /// ```
    /// use gradiance_units::{Dimension, UnitPrefs, AngleUnit};
    /// let prefs = UnitPrefs { angle: AngleUnit::Radians, ..UnitPrefs::default() };
    /// assert_eq!(prefs.display(Dimension::Angle).symbol, "rad");
    /// assert_eq!(UnitPrefs::default().display(Dimension::Angle).symbol, "\u{b0}");
    /// ```
    #[must_use]
    pub const fn display(self, d: Dimension) -> DisplayUnit {
        match (d, self.angle, self.length) {
            (Dimension::Angle, AngleUnit::Degrees, _) => DisplayUnit::DEGREES,
            (Dimension::AngularVelocity, AngleUnit::Degrees, _) => DisplayUnit::DEGREES_PER_S,
            (Dimension::Length, _, length) => DisplayUnit {
                dimension: Dimension::Length,
                symbol: length.name(),
                per_si: length.per_metre(),
            },
            _ => DisplayUnit::canonical(d),
        }
    }
}

/// A unit a field shows a quantity in: its dimension, its symbol, and how
/// many of it make one base-SI unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayUnit {
    /// The quantity's dimension (what a typed unit must match).
    pub dimension: Dimension,
    /// The symbol shown after the number (empty for a pure number).
    pub symbol: &'static str,
    /// Shown units per base-SI unit (`100` for centimetres).
    pub per_si: f64,
}

impl DisplayUnit {
    /// Degrees.
    pub const DEGREES: Self = Self {
        dimension: Dimension::Angle,
        symbol: "\u{b0}",
        per_si: 180.0 / core::f64::consts::PI,
    };
    /// Degrees per second.
    pub const DEGREES_PER_S: Self = Self {
        dimension: Dimension::AngularVelocity,
        symbol: "\u{b0}/s",
        per_si: 180.0 / core::f64::consts::PI,
    };
    /// Revolutions per minute (a hinge motor's speed).
    pub const RPM: Self = Self {
        dimension: Dimension::AngularVelocity,
        symbol: "rpm",
        per_si: 60.0 / core::f64::consts::TAU,
    };

    /// The dimension's canonical SI unit (`m`, `rad`, `N`).
    #[must_use]
    pub const fn canonical(d: Dimension) -> Self {
        Self {
            dimension: d,
            symbol: d.symbol(),
            per_si: 1.0,
        }
    }

    /// A value stored in this unit as base SI (a settings field kept in
    /// degrees reads through [`DisplayUnit::DEGREES`]).
    #[must_use]
    pub fn to_si(self, shown: f64) -> f64 {
        shown / self.per_si
    }

    /// A base-SI value in this unit.
    #[must_use]
    pub fn from_si(self, si: f64) -> f64 {
        si * self.per_si
    }

    /// The text that follows the number in a field: `" m"`, `"°"`, or
    /// nothing for a pure number. A degree sign sits on the number, as it
    /// is written.
    #[must_use]
    pub fn suffix(self) -> String {
        match self.symbol {
            "" => String::new(),
            s if s.starts_with('\u{b0}') => s.to_owned(),
            s => format!(" {s}"),
        }
    }

    /// A base-SI value in this unit with its suffix, trimmed to `decimals`
    /// places (`25 cm`, `90°`).
    #[must_use]
    pub fn format(self, si: f32, decimals: usize) -> String {
        let shown = self.from_si(f64::from(si));
        let number = format!("{shown:.decimals$}");
        let number = if number.contains('.') {
            number.trim_end_matches('0').trim_end_matches('.')
        } else {
            &number
        };
        format!("{number}{}", self.suffix())
    }

    /// Parses a typed entry into a base-SI magnitude for a field showing
    /// this unit: a bare number (or a unitless expression such as `90/4`)
    /// is in this unit; an entry with units must be of this dimension and
    /// converts.
    ///
    /// ```
    /// use gradiance_units::{DisplayUnit, Dimension};
    /// let deg = DisplayUnit::DEGREES;
    /// assert!((deg.parse("90").unwrap() - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    /// assert!((deg.parse("1 rad").unwrap() - 1.0).abs() < 1e-6);
    /// assert!(deg.parse("25 cm").is_err());
    /// ```
    ///
    /// # Errors
    /// [`ExprError`] when the text does not evaluate, or evaluates to a
    /// different dimension ([`ExprError::WrongDimension`]).
    pub fn parse(self, text: &str) -> Result<f32, ExprError> {
        let value = evaluate(text)?;
        let si = if value.dims.is_none() {
            self.to_si(value.si)
        } else if value.dims == self.dimension.dims() {
            value.si
        } else {
            return Err(ExprError::WrongDimension {
                expected: self.expected(),
                found: value.dims.to_string(),
            });
        };
        let si = si as f32;
        if si.is_finite() {
            Ok(si)
        } else {
            Err(ExprError::NotFinite)
        }
    }

    /// What the field expects, for an error message.
    fn expected(self) -> String {
        match self.dimension.symbol() {
            "" => "a plain number".to_owned(),
            s => format!("{s} (or a plain number)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_number_is_in_the_shown_unit() {
        let cm = UnitPrefs {
            length: LengthUnit::Centimetres,
            ..UnitPrefs::default()
        }
        .display(Dimension::Length);
        assert_eq!(cm.parse("25"), Ok(0.25));
        assert_eq!(cm.parse("25 cm"), Ok(0.25));
        assert_eq!(cm.parse("0.25 m"), Ok(0.25));
        assert_eq!(cm.parse("25 cm"), DisplayUnit::canonical(Dimension::Length).parse("25 cm"));
        assert_eq!(DisplayUnit::canonical(Dimension::Length).parse("25"), Ok(25.0));
        assert_eq!(cm.format(0.25, 2), "25 cm");
    }

    #[test]
    fn degrees_and_radians_both_store_radians() {
        let deg = UnitPrefs::default().display(Dimension::Angle);
        let rad = UnitPrefs {
            angle: AngleUnit::Radians,
            ..UnitPrefs::default()
        }
        .display(Dimension::Angle);
        let quarter = core::f32::consts::FRAC_PI_2;
        for (unit, text) in [
            (deg, "90"),
            (deg, "90 deg"),
            (deg, "pi/2 rad"),
            (rad, "pi/2"),
            (rad, "90 deg"),
            (rad, "90\u{b0}"),
            (rad, "0.25 rev"),
        ] {
            let got = unit.parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert!((got - quarter).abs() < 1e-6, "{text} -> {got}");
        }
        assert_eq!(deg.format(quarter, 1), "90\u{b0}");
        assert_eq!(rad.format(quarter, 2), "1.57 rad");
        let spin = UnitPrefs::default().display(Dimension::AngularVelocity);
        assert_eq!(spin.symbol, "\u{b0}/s");
        assert!((spin.parse("60 rpm").unwrap() - core::f32::consts::TAU).abs() < 1e-5);
    }

    #[test]
    fn the_wrong_dimension_is_an_error_that_names_the_field_unit() {
        let err = DisplayUnit::canonical(Dimension::Length)
            .parse("90 deg")
            .unwrap_err();
        assert_eq!(err.to_string(), "expected m (or a plain number), got rad");
        assert!(DisplayUnit::canonical(Dimension::Dimensionless).parse("1 m").is_err());
        assert!(DisplayUnit::canonical(Dimension::Frequency).parse("2 kHz").is_ok());
        assert!(DisplayUnit::canonical(Dimension::AngularVelocity).parse("2 Hz").is_err());
        assert_eq!(DisplayUnit::canonical(Dimension::Force).parse("1e40 kN"), Err(ExprError::NotFinite));
    }

    #[test]
    fn every_dimension_formats_what_it_parses() {
        for prefs in [
            UnitPrefs::default(),
            UnitPrefs {
                angle: AngleUnit::Radians,
                length: LengthUnit::Millimetres,
            },
        ] {
            for d in Dimension::ALL {
                let unit = prefs.display(d);
                assert_eq!(unit.dimension, d);
                let text = unit.format(1.25, 4);
                let back = unit.parse(&text).unwrap_or_else(|e| panic!("{d:?} {text}: {e}"));
                assert!((back - 1.25).abs() < 1e-4, "{d:?}: {text} -> {back}");
            }
        }
    }

    #[test]
    fn names_round_trip() {
        for a in AngleUnit::ALL {
            assert_eq!(AngleUnit::named(a.name()), Some(a));
        }
        for l in LengthUnit::ALL {
            assert_eq!(LengthUnit::named(l.name()), Some(l));
        }
        assert_eq!(AngleUnit::named("grad"), None);
    }
}
