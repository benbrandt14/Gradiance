//! The runtime face of a unit: a [`Dimension`] enum for catalog rows, plot
//! axes, and UI labels, with formatting and (base-SI) parsing.
//!
//! The compile-time face is each quantity's `UNIT` const and its type; this
//! enum is what code carries when the quantity is chosen at runtime (a plotted
//! signal, a catalog entry, an inspector field).

use thiserror::Error;

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
    /// Angular acceleration (`rad/s²`).
    AngularAcceleration,
    /// Moment of inertia about the simulation-plane normal (`kg·m²`).
    MomentOfInertia,
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
    /// Linear impulse (`N·s`).
    Impulse,
    /// Angular impulse (`N·m·s`).
    AngularImpulse,
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
            Self::AngularAcceleration => "rad/s²",
            Self::MomentOfInertia => "kg·m²",
            Self::Force => "N",
            Self::Torque => "N·m",
            Self::Stiffness => "N/m",
            Self::Damping => "N·s/m",
            Self::Frequency => "Hz",
            Self::Impulse => "N·s",
            Self::AngularImpulse => "N·m·s",
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

    /// Parses a base-SI magnitude from text, tolerating a trailing unit symbol
    /// (`"1.5 m"`, `"1.5m"`, or `"1.5"`). Prefix/imperial parsing is a later,
    /// DSL-driven addition (`docs/units-decision.md`); this accepts the
    /// canonical unit only.
    ///
    /// # Errors
    /// Returns [`ParseError`] if the numeric part is not a valid `f32` or a
    /// non-canonical unit trails the number.
    pub fn parse(self, text: &str) -> Result<f32, ParseError> {
        let trimmed = text.trim();
        let number = self
            .symbol()
            .is_empty()
            .then_some(trimmed)
            .or_else(|| trimmed.strip_suffix(self.symbol()).map(str::trim_end))
            .unwrap_or(trimmed);
        number
            .trim()
            .parse::<f32>()
            .map_err(|_| ParseError(text.to_owned()))
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

/// A value string could not be parsed into its dimension.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("could not parse `{0}` as a quantity")]
pub struct ParseError(pub String);
