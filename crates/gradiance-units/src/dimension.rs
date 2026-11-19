//! The runtime face of a unit: a [`Dimension`] enum for catalog rows, plot
//! axes, and UI labels, with formatting and parsing (the grammar is
//! [`crate::expr`]).
//!
//! The compile-time face is each quantity's `UNIT` const and its type; this
//! enum is what code carries when the quantity is chosen at runtime (a plotted
//! signal, a catalog entry, an inspector field).

use crate::display::DisplayUnit;
use crate::expr::{Dims, ExprError};

/// A physical dimension — enough to label a value and pick its canonical unit.
/// One variant per quantity kind the tool measures; `Dimensionless` covers
/// ratios (friction, restitution, gravity scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dimension {
    /// Duration (`s`).
    Time,
    /// Length / distance (`m`).
    Length,
    /// Area (`m²`).
    Area,
    /// Plane angle (`rad`).
    Angle,
    /// Mass (`kg`).
    Mass,
    /// Areal mass density (`kg/m²`).
    Density,
    /// Linear speed (`m/s`).
    Velocity,
    /// Angular speed (`rad/s`).
    AngularVelocity,
    /// Linear acceleration (`m/s²`).
    Acceleration,
    /// Force (`N`).
    Force,
    /// Torque (`N·m`).
    Torque,
    /// Spring stiffness (`N/m`).
    Stiffness,
    /// Linear damping (`N·s/m`).
    Damping,
    /// Frequency (`Hz`).
    Frequency,
    /// Energy (`J`).
    Energy,
    /// Linear momentum (`kg·m/s`).
    Momentum,
    /// Angular momentum (`kg·m²/s`).
    AngularMomentum,
    /// A pure ratio or count — no unit.
    Dimensionless,
}

impl Dimension {
    /// Every dimension, in declaration order.
    pub const ALL: [Self; 18] = [
        Self::Time,
        Self::Length,
        Self::Area,
        Self::Angle,
        Self::Mass,
        Self::Density,
        Self::Velocity,
        Self::AngularVelocity,
        Self::Acceleration,
        Self::Force,
        Self::Torque,
        Self::Stiffness,
        Self::Damping,
        Self::Frequency,
        Self::Energy,
        Self::Momentum,
        Self::AngularMomentum,
        Self::Dimensionless,
    ];

    /// The exponents of the base dimensions (m, kg, s, rad) this dimension
    /// carries: what a typed entry must evaluate to ([`crate::expr`]).
    #[must_use]
    pub const fn dims(self) -> Dims {
        match self {
            Self::Time => Dims::new(0, 0, 1, 0),
            Self::Length => Dims::new(1, 0, 0, 0),
            Self::Area => Dims::new(2, 0, 0, 0),
            Self::Angle => Dims::new(0, 0, 0, 1),
            Self::Mass => Dims::new(0, 1, 0, 0),
            Self::Density => Dims::new(-2, 1, 0, 0),
            Self::Velocity => Dims::new(1, 0, -1, 0),
            Self::AngularVelocity => Dims::new(0, 0, -1, 1),
            Self::Acceleration => Dims::new(1, 0, -2, 0),
            Self::Force => Dims::new(1, 1, -2, 0),
            Self::Torque | Self::Energy => Dims::new(2, 1, -2, 0),
            Self::Stiffness => Dims::new(0, 1, -2, 0),
            Self::Damping => Dims::new(0, 1, -1, 0),
            Self::Frequency => Dims::new(0, 0, -1, 0),
            Self::Momentum => Dims::new(1, 1, -1, 0),
            Self::AngularMomentum => Dims::new(2, 1, -1, 0),
            Self::Dimensionless => Dims::NONE,
        }
    }

    /// The dimension of a `gradiance-units` quantity type, by its type name
    /// (`"Length"`, `"Acceleration2"`): how the reflect grid labels and
    /// parses a newtype field it only sees through reflection. `None` for a
    /// name that is not a quantity, or a quantity no [`Dimension`] names
    /// (`Impulse`).
    #[must_use]
    pub fn of_quantity(type_name: &str) -> Option<Self> {
        Some(match type_name {
            "Time" => Self::Time,
            "Length" | "Displacement" => Self::Length,
            "Area" => Self::Area,
            "Angle" => Self::Angle,
            "Mass" => Self::Mass,
            "Density" => Self::Density,
            "Velocity" | "Velocity2" => Self::Velocity,
            "AngularVelocity" => Self::AngularVelocity,
            "Acceleration" | "Acceleration2" => Self::Acceleration,
            "Force" | "Force2" => Self::Force,
            "Torque" => Self::Torque,
            "Stiffness" => Self::Stiffness,
            "Damping" => Self::Damping,
            "Frequency" => Self::Frequency,
            "Energy" => Self::Energy,
            "Momentum" => Self::Momentum,
            "AngularMomentum" => Self::AngularMomentum,
            _ => return None,
        })
    }

    /// The canonical base-SI unit symbol (empty for [`Dimensionless`]).
    ///
    /// ```
    /// use gradiance_units::Dimension;
    /// assert_eq!(Dimension::Force.symbol(), "N");
    /// assert_eq!(Dimension::Dimensionless.symbol(), "");
    /// ```
    ///
    /// [`Dimensionless`]: Dimension::Dimensionless
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Time => "s",
            Self::Length => "m",
            Self::Area => "m²",
            Self::Angle => "rad",
            Self::Mass => "kg",
            Self::Density => "kg/m²",
            Self::Velocity => "m/s",
            Self::AngularVelocity => "rad/s",
            Self::Acceleration => "m/s²",
            Self::Force => "N",
            Self::Torque => "N·m",
            Self::Stiffness => "N/m",
            Self::Damping => "N·s/m",
            Self::Frequency => "Hz",
            Self::Energy => "J",
            Self::Momentum => "kg·m/s",
            Self::AngularMomentum => "kg·m²/s",
            Self::Dimensionless => "",
        }
    }

    /// Formats a base-SI magnitude with its unit, e.g. `1.50 m` (or `0.30` for
    /// a dimensionless value). Trailing-zero-trimmed to `decimals` places.
    #[must_use]
    pub fn format(self, value: f32, decimals: usize) -> String {
        let number = trim_zeros(&format!("{value:.decimals$}"));
        match self.symbol() {
            "" => number,
            unit => format!("{number} {unit}"),
        }
    }

    /// Parses a typed entry into a base-SI magnitude of this dimension: a
    /// bare number is in the canonical unit, and any unit of this dimension
    /// converts (`"25 cm"` is `0.25` for [`Length`](Self::Length)). The
    /// grammar is [`crate::expr`]; a field with a display preference parses
    /// through [`DisplayUnit::parse`] instead, where a bare number is in the
    /// shown unit.
    ///
    /// ```
    /// use gradiance_units::Dimension;
    /// assert_eq!(Dimension::Length.parse("1.5 m"), Ok(1.5));
    /// assert_eq!(Dimension::Force.parse("2 kN"), Ok(2000.0));
    /// assert!(Dimension::Length.parse("90 deg").is_err());
    /// ```
    ///
    /// # Errors
    /// [`ExprError`] when the text does not evaluate, or evaluates to
    /// another dimension.
    pub fn parse(self, text: &str) -> Result<f32, ExprError> {
        DisplayUnit::canonical(self).parse(text)
    }
}

/// Trims trailing zeros (and a bare decimal point) from a fixed-point string.
fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::*;

    /// `of_quantity` agrees with each quantity type's own `UNIT`: the reflect
    /// grid labels a newtype by name, so a renamed or added type is caught.
    #[test]
    fn of_quantity_matches_every_type_unit() {
        macro_rules! check {
            ($($t:ident),*) => {$(
                let d = Dimension::of_quantity(stringify!($t));
                assert_eq!(d.map(Dimension::symbol), Some($t::UNIT), stringify!($t));
            )*};
        }
        check!(
            Time,
            Length,
            Area,
            Angle,
            Mass,
            Density,
            Velocity,
            AngularVelocity,
            Acceleration,
            Force,
            Torque,
            Stiffness,
            Damping,
            Frequency,
            Energy,
            Momentum,
            AngularMomentum,
            Displacement,
            Velocity2,
            Acceleration2,
            Force2
        );
        // No dimension names an impulse (N·s); it is read-only.
        assert_eq!(Dimension::of_quantity("Impulse"), None);
        assert_eq!(Dimension::of_quantity("f32"), None);
    }

    #[test]
    fn every_dimension_is_listed_once_with_distinct_exponents() {
        for (i, d) in Dimension::ALL.iter().enumerate() {
            for o in &Dimension::ALL[i + 1..] {
                assert_ne!(d, o);
                // Torque and energy share N·m = J; nothing else may.
                if !matches!((d, o), (Dimension::Torque, Dimension::Energy)) {
                    assert_ne!(d.dims(), o.dims(), "{d:?} vs {o:?}");
                }
            }
        }
    }
}
