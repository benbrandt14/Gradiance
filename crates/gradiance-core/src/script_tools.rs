//! The script-tool table: tools a `.scm` file registered, and the message a
//! completed script-tool gesture sends.
//!
//! Lives in `core` because two layers that may not depend on each other both
//! need it. `gradiance-script` *writes* the table (`register-tool`) and runs a
//! tool's body; `gradiance-interaction` *reads* it (how many points the active
//! tool collects) and *sends* [`ScriptToolFired`] when a gesture completes; the
//! UI reads it to draw the palette. Neither layer reaches the other: the
//! script layer publishes, the tool layer reports, and both meet here.
//!
//! A tool is selected through [`ToolState::Script`](crate::states::ToolState::Script),
//! whose index is a position in [`ScriptTools`].

use bevy::prelude::*;

/// One tool a script registered with `(register-tool name points src)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptToolSpec {
    /// The name the palette shows; unique in the table.
    pub name: String,
    /// How many clicks the tool collects before it runs (at least one).
    pub points: usize,
    /// The lisp source run once, when the last point is placed.
    pub source: String,
}

/// Every registered script tool, in registration order. Index `i` is the tool
/// [`ToolState::Script(i)`](crate::states::ToolState::Script) selects.
///
/// Not persisted (like actions and labels): tools live while their script is
/// loaded. Entries are never removed, and re-registering a name updates it in
/// place, so an index stays valid for the whole session.
#[derive(Resource, Debug, Clone, Default)]
pub struct ScriptTools(pub Vec<ScriptToolSpec>);

impl ScriptTools {
    /// Inserts a tool, or updates the one with the same name in place.
    /// Returns its index.
    pub fn upsert(&mut self, spec: ScriptToolSpec) -> usize {
        if let Some(i) = self.0.iter().position(|t| t.name == spec.name) {
            self.0[i] = spec;
            i
        } else {
            self.0.push(spec);
            self.0.len() - 1
        }
    }

    /// The tool at `index`, if registered.
    #[must_use]
    pub fn get(&self, index: u16) -> Option<&ScriptToolSpec> {
        self.0.get(usize::from(index))
    }

    /// The index of the tool named `name`, if registered.
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<u16> {
        self.0
            .iter()
            .position(|t| t.name == name)
            .and_then(|i| u16::try_from(i).ok())
    }
}

/// A script tool's gesture completed: run tool `tool`'s body once, with
/// `points` (world space, in click order) as that run's context.
///
/// Sent by the interaction layer's script-tool driver; read by the script
/// layer, which queues the run. Not a command intent — the run itself emits
/// intents through the ordinary verbs.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ScriptToolFired {
    /// Index into [`ScriptTools`].
    pub tool: u16,
    /// The collected points, world space.
    pub points: Vec<Vec2>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, points: usize) -> ScriptToolSpec {
        ScriptToolSpec {
            name: name.into(),
            points,
            source: String::new(),
        }
    }

    #[test]
    fn re_registering_a_name_keeps_its_index() {
        let mut table = ScriptTools::default();
        assert_eq!(table.upsert(spec("ramp", 2)), 0);
        assert_eq!(table.upsert(spec("peg", 1)), 1);
        assert_eq!(table.upsert(spec("ramp", 3)), 0);
        assert_eq!(table.0.len(), 2);
        assert_eq!(table.get(0).map(|t| t.points), Some(3));
        assert_eq!(table.index_of("peg"), Some(1));
        assert_eq!(table.index_of("nope"), None);
    }
}
