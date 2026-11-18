//! The **quantity catalog** as read-only operations (brief 5-2): list the
//! catalog, ask a row's unit, and read a row off a body, a joint or a pair of
//! bodies. The rows are `gradiance_domain::quantity::QUANTITIES`; adding a
//! row there adds it here with no further code.
//!
//! The values come from the [`QuantitySamples`] mirror, captured once per
//! context (`gradiance_signal::sample_quantities`), so every read in a script
//! run sees the pre-run state, like `body-x`. A target that names nothing
//! reads NaN; an unknown name, or a name read with the wrong number of
//! subjects, is an error that says what to write instead.

use super::{body_of, joint_of, query::query};
use crate::snapshot::SceneSnapshot;
use bevy::prelude::*;
use gradiance_domain::quantity::{
    self, BodySample, JointSample, PhysicalQuantity, QuantitySamples, Subject,
};
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Target, Value};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Quantities>();
    registry.register::<QuantityUnit>();
    registry.register::<QuantityRead>();
    registry.register::<QuantityBetween>();
}

/// The catalog row `name`, or an error that lists the names.
fn row(name: &str) -> Result<&'static PhysicalQuantity, OpError> {
    quantity::quantity(name).ok_or_else(|| {
        OpError::Invalid(format!(
            "unknown quantity \"{name}\"; known: {}",
            quantity::known_names()
        ))
    })
}

/// The error for a row read with the wrong verb: it shows the right one.
fn wrong_verb(row: &PhysicalQuantity) -> OpError {
    let name = row.name;
    OpError::Invalid(match row.subject() {
        Subject::BodyPair => {
            format!("\"{name}\" reads a pair of bodies: (quantity-between \"{name}\" a b)")
        }
        other => format!(
            "\"{name}\" reads one {}: (quantity \"{name}\" i)",
            other.noun()
        ),
    })
}

/// Every quantity name.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Quantities {}

impl Operation for Quantities {
    const META: OpMeta = query(
        "quantity.list",
        "quantities",
        "Every quantity name in the catalog (what quantity, quantity-between and the sensor ports read).",
        "{}",
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let names: Vec<Value> = quantity::QUANTITIES
            .iter()
            .map(|q| Value::from(q.name))
            .collect();
        Ok(OpOutput::none().returning(names))
    }
}

/// A quantity's SI unit.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct QuantityUnit {
    /// The quantity's name, as in (quantities).
    pub name: String,
}

impl Operation for QuantityUnit {
    const META: OpMeta = query(
        "quantity.unit",
        "quantity-unit",
        "The SI unit symbol a quantity reads in (\"\" for a count).",
        r#"{"name": "speed"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().returning(row(&self.name)?.dimension.symbol()))
    }
}

/// A quantity read off one body or one joint.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct QuantityRead {
    /// The quantity's name, as in (quantities).
    pub name: String,
    /// A body (an index in id order, an id, or a point on it), or for a
    /// joint quantity a joint (an index in id order or an id).
    pub i: Target,
}

impl Operation for QuantityRead {
    const META: OpMeta = query(
        "quantity.read",
        "quantity",
        "A catalog quantity of the i-th body, or of the i-th joint for a joint-* quantity, in SI; NaN if out of range or not simulating.",
        r#"{"name": "speed", "i": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let row = row(&self.name)?;
        let scene = ctx.get::<SceneSnapshot>()?;
        let samples = ctx.get::<QuantitySamples>()?;
        let value = match row.subject() {
            Subject::Body => body_of(ctx, scene, &self.i)?
                .map(|b| samples.bodies.get(&b.id).copied().unwrap_or_default())
                .and_then(|b: BodySample| row.read_body(&b)),
            Subject::Joint => joint_of(ctx, scene, &self.i)?
                .map(|id| samples.joints.get(&id).copied().unwrap_or_default())
                .and_then(|j: JointSample| row.read_joint(&j)),
            Subject::BodyPair => return Err(wrong_verb(row)),
        };
        Ok(OpOutput::none().returning(value.map_or(f64::NAN, f64::from)))
    }
}

/// A quantity read off two bodies.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct QuantityBetween {
    /// The quantity's name, as in (quantities).
    pub name: String,
    /// The first body.
    pub a: Target,
    /// The second body.
    pub b: Target,
}

impl Operation for QuantityBetween {
    const META: OpMeta = query(
        "quantity.between",
        "quantity-between",
        "A two-body catalog quantity (distance) between bodies a and b, in SI; NaN if either is out of range.",
        r#"{"name": "distance", "a": 0, "b": 1}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let row = row(&self.name)?;
        if row.subject() != Subject::BodyPair {
            return Err(wrong_verb(row));
        }
        let scene = ctx.get::<SceneSnapshot>()?;
        let samples = ctx.get::<QuantitySamples>()?;
        let sample = |target: &Target| -> Result<Option<BodySample>, OpError> {
            Ok(body_of(ctx, scene, target)?
                .map(|b| samples.bodies.get(&b.id).copied().unwrap_or_default()))
        };
        let value = match (sample(&self.a)?, sample(&self.b)?) {
            (Some(a), Some(b)) => row.read_pair(&a, &b),
            _ => None,
        };
        Ok(OpOutput::none().returning(value.map_or(f64::NAN, f64::from)))
    }
}
