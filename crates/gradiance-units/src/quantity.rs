//! The typed quantities: newtypes over `f32` (scalars) and `Vec2` (planar
//! vectors), each holding a value in the base SI unit.
//!
//! Only the arithmetic the tool actually uses is implemented, so the common
//! relations (`Force = Mass · Acceleration`, `Area = Length · Length`,
//! `Mass = Density · Area`) are dimension-checked without a full
//! dimensional-analysis engine.

use bevy::math::Vec2;
use bevy::reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Generates a scalar quantity newtype (`#[repr(transparent)]` over `f32`,
/// base SI) with its constructor, accessor, unit symbol, and the pointwise
/// arithmetic every quantity shares (`+`, `-`, unary `-`, scale by `f32`,
/// and ratio `Self / Self -> f32`).
macro_rules! scalar_quantity {
    ($(#[$doc:meta])+ $name:ident, $ctor:ident, $unit:literal) => {
        $(#[$doc])+
        #[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Default, Serialize, Deserialize, Reflect)]
        #[repr(transparent)]
        pub struct $name(pub f32);

        impl $name {
            #[doc = concat!("Constructs from a magnitude in the base SI unit (`", $unit, "`).")]
            pub const fn $ctor(value: f32) -> Self {
                Self(value)
            }

            /// The magnitude in the base SI unit.
            pub const fn value(self) -> f32 {
                self.0
            }

            #[doc = concat!("The canonical base-SI unit symbol (`", $unit, "`).")]
            pub const UNIT: &'static str = $unit;
        }

        impl core::ops::Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl core::ops::Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl core::ops::Neg for $name {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        impl core::ops::Mul<f32> for $name {
            type Output = Self;
            fn mul(self, scale: f32) -> Self {
                Self(self.0 * scale)
            }
        }

        impl core::ops::Div<f32> for $name {
            type Output = Self;
            fn div(self, scale: f32) -> Self {
                Self(self.0 / scale)
            }
        }

        impl core::ops::Div for $name {
            type Output = f32;
            fn div(self, rhs: Self) -> f32 {
                self.0 / rhs.0
            }
        }
    };
}

/// Generates a planar vector quantity newtype over [`Vec2`] (base SI per
/// component) with its constructor, accessor, magnitude, and vector
/// arithmetic.
macro_rules! vector_quantity {
    ($(#[$doc:meta])+ $name:ident, $mag:ident, $unit:literal) => {
        $(#[$doc])+
        #[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize, Reflect)]
        #[repr(transparent)]
        pub struct $name(pub Vec2);

        impl $name {
            #[doc = concat!("Constructs from a vector in the base SI unit (`", $unit, "` per component).")]
            pub const fn new(value: Vec2) -> Self {
                Self(value)
            }

            /// The vector in the base SI unit.
            pub const fn value(self) -> Vec2 {
                self.0
            }

            #[doc = concat!("The scalar magnitude, as a [`", stringify!($mag), "`].")]
            pub fn magnitude(self) -> $mag {
                $mag(self.0.length())
            }

            #[doc = concat!("The canonical base-SI unit symbol (`", $unit, "`).")]
            pub const UNIT: &'static str = $unit;
        }

        impl core::ops::Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl core::ops::Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl core::ops::Neg for $name {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        impl core::ops::Mul<f32> for $name {
            type Output = Self;
            fn mul(self, scale: f32) -> Self {
                Self(self.0 * scale)
            }
        }
    };
}

scalar_quantity! {
    /// A duration, in seconds.
    Time, seconds, "s"
}
scalar_quantity! {
    /// A length / distance, in metres.
    ///
    /// ```
    /// use gradiance_units::Length;
    /// let l = Length::metres(1.5);
    /// assert_eq!(l.value(), 1.5);
    /// assert_eq!(Length::UNIT, "m");
    /// ```
    Length, metres, "m"
}
scalar_quantity! {
    /// An area, in square metres.
    Area, square_metres, "m²"
}
scalar_quantity! {
    /// A plane angle, in radians.
    Angle, radians, "rad"
}
scalar_quantity! {
    /// A mass, in kilograms.
    Mass, kilograms, "kg"
}
scalar_quantity! {
    /// An **areal** mass density (2D), in kilograms per square metre.
    ///
    /// Mass is `density · area` ([`mass_of`](crate::mass::mass_of)); a future
    /// jump to volumetric density (kg/m³) is localized there — see
    /// `docs/units-decision.md`.
    Density, kg_per_square_metre, "kg/m²"
}
scalar_quantity! {
    /// A linear speed, in metres per second.
    Velocity, metres_per_second, "m/s"
}
scalar_quantity! {
    /// An angular speed, in radians per second.
    AngularVelocity, radians_per_second, "rad/s"
}
scalar_quantity! {
    /// A linear acceleration, in metres per second squared.
    Acceleration, metres_per_second_squared, "m/s²"
}
scalar_quantity! {
    /// A force, in newtons.
    Force, newtons, "N"
}
scalar_quantity! {
    /// A torque, in newton-metres.
    Torque, newton_metres, "N·m"
}
scalar_quantity! {
    /// A spring stiffness, in newtons per metre.
    Stiffness, newtons_per_metre, "N/m"
}
scalar_quantity! {
    /// A linear damping coefficient, in newton-seconds per metre.
    Damping, newton_seconds_per_metre, "N·s/m"
}
scalar_quantity! {
    /// A frequency, in hertz.
    Frequency, hertz, "Hz"
}
scalar_quantity! {
    /// A linear impulse, in newton-seconds (force integrated over time; a
    /// contact's `normal_impulse`).
    Impulse, newton_seconds, "N·s"
}
scalar_quantity! {
    /// Energy (kinetic / potential / work), in joules.
    Energy, joules, "J"
}
scalar_quantity! {
    /// Linear momentum (mass · velocity), in kilogram-metres per second.
    Momentum, kilogram_metres_per_second, "kg·m/s"
}

vector_quantity! {
    /// A planar displacement / position, in metres.
    Displacement, Length, "m"
}
vector_quantity! {
    /// A planar velocity, in metres per second.
    Velocity2, Velocity, "m/s"
}
vector_quantity! {
    /// A planar acceleration, in metres per second squared.
    Acceleration2, Acceleration, "m/s²"
}
vector_quantity! {
    /// A planar force, in newtons.
    Force2, Force, "N"
}
vector_quantity! {
    /// A planar impulse, in newton-seconds (a net contact impulse).
    Impulse2, Impulse, "N·s"
}

// --- cross-dimension relations (only those the tool uses) ---

impl core::ops::Mul<Length> for Length {
    type Output = Area;
    /// `Length · Length = Area`.
    fn mul(self, rhs: Length) -> Area {
        Area(self.0 * rhs.0)
    }
}

impl core::ops::Mul<Acceleration> for Mass {
    type Output = Force;
    /// Newton's second law: `Mass · Acceleration = Force`.
    fn mul(self, rhs: Acceleration) -> Force {
        Force(self.0 * rhs.0)
    }
}

impl core::ops::Mul<Area> for Density {
    type Output = Mass;
    /// Areal density times area is mass (the 2D [`mass_of`](crate::mass::mass_of)
    /// relation).
    fn mul(self, rhs: Area) -> Mass {
        Mass(self.0 * rhs.0)
    }
}

impl core::ops::Mul<Length> for Stiffness {
    type Output = Force;
    /// Hooke's law: `Stiffness · Length = Force`.
    fn mul(self, rhs: Length) -> Force {
        Force(self.0 * rhs.0)
    }
}

impl core::ops::Mul<Acceleration2> for Mass {
    type Output = Force2;
    /// Newton's second law, planar: `Mass · Acceleration2 = Force2`.
    fn mul(self, rhs: Acceleration2) -> Force2 {
        Force2(rhs.0 * self.0)
    }
}

impl core::ops::Div<Time> for Impulse {
    type Output = Force;
    /// Impulse over the timestep is force: `Impulse / Time = Force` (a
    /// contact impulse ÷ fixed dt reads as the contact force).
    fn div(self, rhs: Time) -> Force {
        Force(self.0 / rhs.0)
    }
}
