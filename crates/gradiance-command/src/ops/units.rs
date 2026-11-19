//! **Units in scripts** (brief 5-4): numbers in the units an author thinks
//! in, converted to SI at the authoring boundary.
//!
//! Every verb still reads and writes SI (metres, radians), so existing
//! scripts are unchanged; these three convert *into* that. A unit is any
//! text the typed-field parser reads (`cm`, `km/h`, `deg/s`, `N m`): one
//! grammar, `gradiance_units::expr`, for a field and a script. A quantity
//! name from the catalog (`speed`, `height`) stands for its SI unit in
//! `convert`, so a value can be checked against what it is meant to be.
//!
//! All three are pure: they read no scene state, so they need no snapshot,
//! and nothing about units reaches the per-frame kernel. A unit the parser
//! does not read, or a conversion between two dimensions, is an
//! [`OpError::Invalid`], which the console shows as a script error.

use super::query::query;
use bevy::prelude::*;
use gradiance_domain::quantity;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation};
use gradiance_units::ExprError;
use gradiance_units::convert::{self, known_units, si_unit};
use gradiance_units::expr::{self, evaluate};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<UnitsSi>();
    registry.register::<UnitsConvert>();
    registry.register::<UnitsOf>();
}

/// A parser error as a script error: it quotes the unit, and an unknown
/// unit lists the known ones.
fn invalid(unit: &str, error: &ExprError) -> OpError {
    OpError::Invalid(match error {
        ExprError::UnknownUnit(_) => {
            format!("unit \"{unit}\": {error}; units: {}", known_units())
        }
        _ => format!("unit \"{unit}\": {error}"),
    })
}

/// A unit, or a catalog quantity name standing for its SI unit.
fn unit_or_quantity(text: &str) -> Result<expr::Value, OpError> {
    if let Some(row) = quantity::quantity(text) {
        return Ok(si_unit(row.dimension));
    }
    evaluate(text).map_err(|e| invalid(text, &e))
}

/// A number given in a unit, in SI.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UnitsSi {
    /// The number, in `unit`.
    pub value: f64,
    /// Its unit: `cm`, `deg`, `km/h`, `kN`, … (any unit a field takes).
    pub unit: String,
}

impl Operation for UnitsSi {
    const META: OpMeta = query(
        "units.si",
        "q",
        "A number in a unit, in SI: (q 25 \"cm\") is 0.25, (q 90 \"deg\") is pi/2. A unit the parser does not read is an error.",
        r#"{"value": 25, "unit": "cm"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let si = convert::to_si(self.value, &self.unit).map_err(|e| invalid(&self.unit, &e))?;
        Ok(OpOutput::none().returning(si))
    }
}

/// A number converted between two units of one dimension.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UnitsConvert {
    /// The number, in `from`.
    pub value: f64,
    /// Its unit, or a quantity name for that quantity's SI unit.
    pub from: String,
    /// The unit wanted, or a quantity name for that quantity's SI unit.
    pub to: String,
}

impl Operation for UnitsConvert {
    const META: OpMeta = query(
        "units.convert",
        "convert",
        "A number from one unit to another of the same dimension: (convert 0.25 \"m\" \"cm\") is 25. A quantity name stands for its SI unit, so (convert 90 \"km/h\" \"speed\") is 25 and (convert 90 \"deg\" \"height\") is an error.",
        r#"{"value": 0.25, "from": "m", "to": "cm"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let from = unit_or_quantity(&self.from)?;
        let to = unit_or_quantity(&self.to)?;
        let value = convert::convert_values(self.value, from, to).map_err(|e| {
            OpError::Invalid(format!(
                "cannot convert \"{}\" to \"{}\": {e}",
                self.from, self.to
            ))
        })?;
        Ok(OpOutput::none().returning(value))
    }
}

/// The SI unit of what a unit measures.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UnitsOf {
    /// A unit (`km/h`) or a value with one (`25 cm`).
    pub unit: String,
}

impl Operation for UnitsOf {
    const META: OpMeta = query(
        "units.of",
        "unit-of",
        "The SI unit symbol of what a unit measures, as quantity-unit reports it: (unit-of \"km/h\") is \"m/s\" (\"\" for a plain number).",
        r#"{"unit": "km/h"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let symbol = convert::si_symbol(&self.unit).map_err(|e| invalid(&self.unit, &e))?;
        Ok(OpOutput::none().returning(symbol))
    }
}
