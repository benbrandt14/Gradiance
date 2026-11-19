//! The last operation, for Adjust Last Operation (brief U-11).
//!
//! Each doorway that applies an operation which records an undo step
//! ([`OpEntry::records_step`]) notes the call in the [`OpJournal`]: its id,
//! its params and the selection it read. The command layer drains the journal
//! when it dispatches; when exactly one journalled call produced exactly one
//! undo step, that call becomes the [`LastOp`]. Anything else that moves the
//! history (another step, an undo, a redo, a load, a settings step, a
//! simulation run) clears it.
//!
//! Both are **derived** state: never saved, never in scene content, never in
//! an undo record. Adjusting goes through the existing seams only: the
//! command layer undoes the step through the stack and re-invokes the
//! operation through [`invoke_as`], so the new step is an ordinary one.

use crate::context::OpContext;
use crate::output::apply;
use crate::registry::{OpEntry, OpError, OpRegistry};
use crate::target::SelectionMirror;
use bevy::prelude::*;
use serde_json::Value;

/// One applied call of an operation that records an undo step.
#[derive(Debug, Clone, PartialEq)]
pub struct OpCall {
    /// The operation's id.
    pub id: &'static str,
    /// The named params it was called with.
    pub params: Value,
    /// The selection its context held, or `None` where the doorway has none
    /// (the Lisp doorway). A re-invocation reads this selection, not the
    /// current one, so `"selection"` names the same objects again.
    pub selection: Option<SelectionMirror>,
}

/// The calls applied since the command layer last dispatched, oldest first.
#[derive(Resource, Debug, Default)]
pub struct OpJournal(pub Vec<OpCall>);

/// The operation that made the newest undo step, while that step is still
/// the newest: what the Adjust Last Operation panel shows.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct LastOp(pub Option<LastOpRecord>);

/// [`LastOp`]'s contents.
#[derive(Debug, Clone, PartialEq)]
pub struct LastOpRecord {
    /// The call, with the params that made the step.
    pub call: OpCall,
    /// The history revision right after the step (the command stack's
    /// counter). The record is valid while the stack is still at it.
    pub step: u64,
}

impl OpEntry {
    /// Whether a call of this operation records one undo step of its own:
    /// an edit with an undo label. These are the calls the journal keeps.
    pub fn records_step(&self) -> bool {
        self.meta.seam == crate::Seam::Edit && self.meta.undo.is_some()
    }
}

/// Notes one applied call in the [`OpJournal`] if its operation records a
/// step. Every doorway calls it once per applied call ([`crate::invoke`] does
/// so itself).
pub fn journal(
    world: &mut World,
    entry: &OpEntry,
    params: Value,
    selection: Option<SelectionMirror>,
) {
    if !entry.records_step() {
        return;
    }
    if let Some(mut journal) = world.get_resource_mut::<OpJournal>() {
        journal.0.push(OpCall {
            id: entry.meta.id,
            params,
            selection,
        });
    }
}

/// Lowers and applies one operation as [`crate::invoke`] does, but against
/// `selection` instead of the live one (`None` withdraws it, as the Lisp
/// doorway does). The re-invocation path of Adjust Last Operation.
pub fn invoke_as(
    world: &mut World,
    id: &str,
    params: Value,
    selection: Option<SelectionMirror>,
) -> Result<Option<Value>, OpError> {
    let entry = world
        .get_resource::<OpRegistry>()
        .and_then(|registry| registry.get(id).cloned())
        .ok_or_else(|| OpError::Unknown(id.to_owned()))?;
    let mut ctx = OpContext::build_for(world, &|op| op == entry.meta.id);
    match &selection {
        Some(mirror) => ctx.insert(mirror.clone()),
        None => ctx.remove::<SelectionMirror>(),
    }
    let output = entry.lower(params.clone(), &ctx)?;
    let value = apply(world, entry.meta.id, output);
    journal(world, &entry, params, selection);
    Ok(value)
}

/// Writes `value` at the dotted `path` inside `params` (`"w"`, `"step.0"`):
/// the edit an adjustment makes. The field must exist, and the new value
/// takes the old one's JSON kind: a whole number is written as an integer
/// (so a count field decodes it; a fraction there is the operation's decode
/// error), a number into a boolean is `true` unless zero.
pub fn set_param(params: &mut Value, path: &str, value: &Value) -> Result<(), OpError> {
    let mut slot = params;
    for segment in path.split('.') {
        let next = match slot {
            Value::Object(fields) => fields.get_mut(segment),
            Value::Array(items) => segment.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
        slot =
            next.ok_or_else(|| OpError::Invalid(format!("the last operation has no `{path}`")))?;
    }
    *slot = match (&*slot, value) {
        // A whole number is written as one, so a count field decodes it;
        // `1.0` and `1` mean the same to a decimal field.
        (Value::Number(_), Value::Number(n))
            if n.as_f64()
                .is_some_and(|x| x.fract() == 0.0 && x.abs() < 9.0e15) =>
        {
            Value::from(n.as_f64().map_or(0, |x| x as i64))
        }
        (Value::Bool(_), Value::Number(n)) => Value::Bool(n.as_f64().is_some_and(|x| x != 0.0)),
        (Value::Bool(_), Value::Bool(b)) => Value::Bool(*b),
        // A number for a number; a target written as an index may be renamed
        // by an id and back.
        (Value::Number(_) | Value::String(_), Value::Number(_) | Value::String(_)) => value.clone(),
        (old, new) => {
            return Err(OpError::Invalid(format!(
                "`{path}` holds {old}, which {new} cannot replace"
            )));
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_param_is_set_by_its_path_in_its_own_kind() {
        let mut params = json!({"w": 1.5, "count": 3, "step": [1.0, 0.0], "on": false});
        set_param(&mut params, "w", &json!(2.25)).ok();
        set_param(&mut params, "count", &json!(5.0)).ok();
        set_param(&mut params, "step.1", &json!(-0.5)).ok();
        set_param(&mut params, "on", &json!(1)).ok();
        assert_eq!(
            params,
            json!({"w": 2.25, "count": 5, "step": [1.0, -0.5], "on": true})
        );
    }

    #[test]
    fn a_missing_path_or_a_wrong_kind_is_an_error() {
        let mut params = json!({"w": 1.5, "step": [1.0, 0.0]});
        assert!(set_param(&mut params, "h", &json!(1)).is_err());
        assert!(set_param(&mut params, "step.2", &json!(1)).is_err());
        assert!(set_param(&mut params, "step", &json!(1)).is_err());
        assert_eq!(params, json!({"w": 1.5, "step": [1.0, 0.0]}));
    }
}
