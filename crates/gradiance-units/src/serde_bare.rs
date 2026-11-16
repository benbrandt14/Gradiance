//! Serde adapters that store a typed quantity as its **bare** base-SI value.
//!
//! A quantity newtype's derived serde impl writes the newtype wrapper: a
//! `Length(0.5)` field saves as `near: (0.5)` in RON, where the `f32` it
//! replaced saved as `near: 0.5`, and neither spelling loads into the other.
//! Retyping an authored field from `f32` to a quantity therefore keeps the
//! scene format only with one of these adapters on the field:
//!
//! ```ignore
//! #[serde(with = "gradiance_units::serde_bare::scalar")]
//! pub near: Length,
//! ```
//!
//! **Serialize** writes the bare value (`0.5`, `(1.0, 2.0)`), byte-identical to
//! the field's pre-retype `f32`/`Vec2`. **Deserialize** is lenient: it accepts
//! the bare value *and* the wrapped spelling (`(0.5)`, `((1.0, 2.0))`), so a
//! hand-edited file in either form loads.
//!
//! The joint motor fields (`target_velocity`, `max_torque`, `max_force`) and
//! `JointCommon::break_force` were typed before these adapters existed and
//! keep the wrapped spelling on disk; unifying the two is a format change
//! (`docs/units-decision.md` § ruling 2).

use crate::quantity::{ScalarQuantity, VectorQuantity};
use bevy::math::Vec2;
use core::fmt;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A base-SI `f32` read from either `0.5` or `(0.5)`.
struct LenientF32(f32);

impl<'de> Deserialize<'de> for LenientF32 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(LenientF32Visitor)
    }
}

struct LenientF32Visitor;

impl<'de> Visitor<'de> for LenientF32Visitor {
    type Value = LenientF32;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number, or a number wrapped in one tuple `(x)`")
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the stored value is f32; formats hand floats over as f64"
    )]
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(LenientF32(v as f32))
    }

    fn visit_f32<E: de::Error>(self, v: f32) -> Result<Self::Value, E> {
        Ok(LenientF32(v))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "an integer literal in a float slot, as RON allows for f32"
    )]
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(LenientF32(v as f32))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "an integer literal in a float slot, as RON allows for f32"
    )]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(LenientF32(v as f32))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        f32::deserialize(deserializer).map(LenientF32)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let value: f32 = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::invalid_length(2, &self));
        }
        Ok(LenientF32(value))
    }
}

/// One element of a vector's outer tuple: a bare component (`1.0`, the bare
/// `(1.0, 2.0)` spelling) or the whole vector (the wrapped `((1.0, 2.0))`).
enum VecElement {
    Component(f32),
    Whole(Vec2),
}

impl<'de> Deserialize<'de> for VecElement {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(VecElementVisitor)
    }
}

struct VecElementVisitor;

impl<'de> Visitor<'de> for VecElementVisitor {
    type Value = VecElement;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a vector component or a two-component vector")
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        LenientF32Visitor
            .visit_f64(v)
            .map(|c| VecElement::Component(c.0))
    }

    fn visit_f32<E: de::Error>(self, v: f32) -> Result<Self::Value, E> {
        Ok(VecElement::Component(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        LenientF32Visitor
            .visit_i64(v)
            .map(|c| VecElement::Component(c.0))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        LenientF32Visitor
            .visit_u64(v)
            .map(|c| VecElement::Component(c.0))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let x: f32 = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let y: f32 = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::invalid_length(3, &self));
        }
        Ok(VecElement::Whole(Vec2::new(x, y)))
    }
}

/// A base-SI `Vec2` read from either `(1.0, 2.0)` or `((1.0, 2.0))`.
struct LenientVec2(Vec2);

impl<'de> Deserialize<'de> for LenientVec2 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(LenientVec2Visitor)
    }
}

struct LenientVec2Visitor;

impl<'de> Visitor<'de> for LenientVec2Visitor {
    type Value = LenientVec2;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a vector `(x, y)`, or one wrapped in a tuple `((x, y))`")
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        Vec2::deserialize(deserializer).map(LenientVec2)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let first: VecElement = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let value = match first {
            VecElement::Whole(v) => v,
            VecElement::Component(x) => {
                let y: f32 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Vec2::new(x, y)
            }
        };
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::invalid_length(3, &self));
        }
        Ok(LenientVec2(value))
    }
}

/// `#[serde(with = "gradiance_units::serde_bare::scalar")]` — a scalar
/// quantity stored as its bare `f32`.
pub mod scalar {
    use super::{Deserialize, Deserializer, LenientF32, ScalarQuantity, Serialize, Serializer};

    /// Writes the bare base-SI value.
    ///
    /// # Errors
    /// Whatever the serializer reports.
    pub fn serialize<T: ScalarQuantity, S: Serializer>(
        value: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.si().serialize(serializer)
    }

    /// Reads `x` or `(x)`.
    ///
    /// # Errors
    /// Neither spelling parsed.
    pub fn deserialize<'de, T: ScalarQuantity, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<T, D::Error> {
        LenientF32::deserialize(deserializer).map(|v| T::from_si(v.0))
    }
}

/// `#[serde(with = "gradiance_units::serde_bare::vec2")]` — a vector quantity
/// stored as its bare `Vec2` (`(x, y)`).
pub mod vec2 {
    use super::{Deserialize, Deserializer, LenientVec2, Serialize, Serializer, VectorQuantity};

    /// Writes the bare base-SI vector.
    ///
    /// # Errors
    /// Whatever the serializer reports.
    pub fn serialize<T: VectorQuantity, S: Serializer>(
        value: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.si().serialize(serializer)
    }

    /// Reads `(x, y)` or `((x, y))`.
    ///
    /// # Errors
    /// Neither spelling parsed.
    pub fn deserialize<'de, T: VectorQuantity, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<T, D::Error> {
        LenientVec2::deserialize(deserializer).map(|v| T::from_si(v.0))
    }
}

/// `#[serde(with = "gradiance_units::serde_bare::option_pair")]` — an optional
/// `[min, max]` pair of a scalar quantity (joint limits, a spring's range),
/// stored as the bare `Option<[f32; 2]>` (`Some((-1.0, 1.0))`).
pub mod option_pair {
    use super::{Deserialize, Deserializer, LenientF32, ScalarQuantity, Serialize, Serializer};

    /// Writes `None` or `Some((min, max))` with bare components.
    ///
    /// # Errors
    /// Whatever the serializer reports.
    pub fn serialize<T: ScalarQuantity, S: Serializer>(
        value: &Option<[T; 2]>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(|[a, b]| [a.si(), b.si()]).serialize(serializer)
    }

    /// Reads `Some((a, b))` with each component bare or wrapped.
    ///
    /// # Errors
    /// The pair did not parse.
    pub fn deserialize<'de, T: ScalarQuantity, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<[T; 2]>, D::Error> {
        let pair: Option<(LenientF32, LenientF32)> = Option::deserialize(deserializer)?;
        Ok(pair.map(|(a, b)| [T::from_si(a.0), T::from_si(b.0)]))
    }
}

#[cfg(test)]
mod tests {
    // float_cmp: values round-trip exactly through the text format.
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::{Angle, Displacement, Length};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Typed {
        #[serde(with = "scalar")]
        near: Length,
        #[serde(with = "vec2")]
        at: Displacement,
        #[serde(with = "option_pair")]
        limits: Option<[Angle; 2]>,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Untyped {
        near: f32,
        at: Vec2,
        limits: Option<[f32; 2]>,
    }

    fn pretty<T: Serialize>(value: &T) -> String {
        ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new()).unwrap_or_default()
    }

    fn typed() -> Typed {
        Typed {
            near: Length(0.5),
            at: Displacement(Vec2::new(1.25, -2.0)),
            limits: Some([Angle(-1.0), Angle(1.5)]),
        }
    }

    #[test]
    fn serializes_byte_identical_to_the_untyped_field() {
        let untyped = Untyped {
            near: 0.5,
            at: Vec2::new(1.25, -2.0),
            limits: Some([-1.0, 1.5]),
        };
        let text = pretty(&typed());
        assert_eq!(text, pretty(&untyped));
        assert!(text.contains("near: 0.5,"), "{text}");
        assert!(text.contains("at: (1.25, -2.0),"), "{text}");
    }

    #[test]
    fn loads_the_bare_spelling() {
        let text = "(near: 0.5, at: (1.25, -2.0), limits: Some((-1.0, 1.5)))";
        assert_eq!(ron::from_str::<Typed>(text).ok(), Some(typed()));
    }

    #[test]
    fn loads_the_wrapped_spelling() {
        let text = "(near: (0.5), at: ((1.25, -2.0)), limits: Some(((-1.0), (1.5))))";
        assert_eq!(ron::from_str::<Typed>(text).ok(), Some(typed()));
    }

    #[test]
    fn loads_integer_literals_and_none() {
        let text = "(near: 2, at: (1, -2), limits: None)";
        let parsed = ron::from_str::<Typed>(text).ok();
        assert_eq!(
            parsed,
            Some(Typed {
                near: Length(2.0),
                at: Displacement(Vec2::new(1.0, -2.0)),
                limits: None,
            })
        );
    }

    #[test]
    fn rejects_malformed_values() {
        for text in [
            "(near: (0.5, 1.0), at: (1.0, 2.0), limits: None)",
            "(near: 0.5, at: (1.0, 2.0, 3.0), limits: None)",
            "(near: 0.5, at: (1.0), limits: None)",
            "(near: \"x\", at: (1.0, 2.0), limits: None)",
        ] {
            assert!(ron::from_str::<Typed>(text).is_err(), "{text} parsed");
        }
    }
}
