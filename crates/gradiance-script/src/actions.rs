//! The action table: named, labelled lisp bodies the editor's menus surface.
//!
//! One table holds two kinds of entry:
//!
//! - **built-in** entries, seeded here — the context-menu commands whose
//!   behaviour is expressible as existing verbs. The menu renders them from
//!   this table instead of hand-wiring a button each, and a script reaches
//!   the same behaviour by evaluating the same body (the body *is* ordinary
//!   script; see [`ScriptActions::builtin`]).
//! - **user** entries, added by the `register-action` verb from a `.scm`.
//!
//! Invoking either kind is the same thing: the UI submits the entry's source
//! through [`ScriptInputs::invoke`](crate::bridge::ScriptInputs::invoke) with
//! the world-space click point, which the run reads back with `(action-x)` /
//! `(action-y)`. Pure data — no ECS reads, no steel — so a replacement shell
//! (`docs/orchestration.md` § Future intent 1) can read it as is.

use bevy::prelude::Resource;
use gradiance_core::constants::GRAVITY;

/// Stable names of the built-in actions. Names are an API (a menu, a test or
/// a future remote client refers to an entry by them); labels are prose.
pub mod builtin {
    /// Put gravity back to the default side-view vector.
    pub const RESTORE_GRAVITY: &str = "restore-gravity";
    /// Zero in-plane gravity (top-down view), giving the back plane some
    /// friction if it has none.
    pub const TOP_DOWN: &str = "top-down";
}

/// A named action: a label plus the lisp source it runs.
///
/// Invoking one submits its source through the same `ScriptInputs` seam a
/// REPL line uses, so an action is governed exactly like any other script —
/// no new mutation path.
#[derive(Debug, Clone)]
pub struct ScriptAction {
    /// The label the editor shows.
    pub label: String,
    /// The lisp source the action runs when invoked.
    pub source: String,
    /// The built-in name for a seeded entry; `None` for a user's
    /// `register-action` entry.
    pub builtin: Option<&'static str>,
    /// Hover text (empty when there is none).
    pub hint: String,
}

impl ScriptAction {
    /// A user-registered action (what `register-action` produces).
    #[must_use]
    pub fn user(label: String, source: String) -> Self {
        Self {
            label,
            source,
            builtin: None,
            hint: String::new(),
        }
    }

    fn seeded(name: &'static str, label: &str, source: String, hint: &str) -> Self {
        Self {
            label: label.to_string(),
            source,
            builtin: Some(name),
            hint: hint.to_string(),
        }
    }
}

/// The built-in entries, in menu order.
///
/// Each body is a plain verb call, so what the menu does and what a script
/// can do are the same text. `sim-set` writes `SimSettings`, which is scene
/// content: `command::commit_settings_edits` records the run's writes as one
/// undo step, exactly as it did for the hand-wired buttons.
#[must_use]
pub fn builtin_seed() -> Vec<ScriptAction> {
    vec![
        ScriptAction::seeded(
            builtin::RESTORE_GRAVITY,
            "Restore gravity (side view)",
            format!(
                "(begin (sim-set \"gravity.x\" {}) (sim-set \"gravity.y\" {}))",
                GRAVITY.x, GRAVITY.y
            ),
            "",
        ),
        ScriptAction::seeded(
            builtin::TOP_DOWN,
            "Top-down mode (gravity into screen)",
            "(begin (sim-set \"gravity.x\" 0) (sim-set \"gravity.y\" 0) \
             (when (<= (sim-get \"plane_friction\") 0) \
             (sim-set \"plane_friction\" 0.3)))"
                .to_string(),
            "zeroes in-plane gravity; give the plane some friction",
        ),
    ]
}

/// The action table the UI surfaces (e.g. the context menu): the built-in
/// seed followed by user entries. `register-action` upserts user entries by
/// label, so re-running a registration script is idempotent; it never
/// replaces a built-in.
#[derive(Resource, Debug)]
pub struct ScriptActions(pub Vec<ScriptAction>);

impl Default for ScriptActions {
    fn default() -> Self {
        Self(builtin_seed())
    }
}

impl ScriptActions {
    /// Inserts or updates a user action by label.
    pub fn upsert(&mut self, action: ScriptAction) {
        if let Some(existing) = self
            .0
            .iter_mut()
            .find(|a| a.builtin.is_none() && a.label == action.label)
        {
            existing.source = action.source;
        } else {
            self.0.push(action);
        }
    }

    /// The built-in entry named `name`, if seeded.
    #[must_use]
    pub fn builtin(&self, name: &str) -> Option<&ScriptAction> {
        self.0.iter().find(|a| a.builtin == Some(name))
    }

    /// The built-in entries, in seed order.
    pub fn builtins(&self) -> impl Iterator<Item = &ScriptAction> {
        self.0.iter().filter(|a| a.builtin.is_some())
    }

    /// The user-registered entries, in registration order.
    pub fn user(&self) -> impl Iterator<Item = &ScriptAction> {
        self.0.iter().filter(|a| a.builtin.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_names_are_unique() {
        let seed = builtin_seed();
        for (i, a) in seed.iter().enumerate() {
            assert!(
                seed[i + 1..].iter().all(|b| b.builtin != a.builtin),
                "duplicate built-in {:?}",
                a.builtin
            );
        }
    }

    #[test]
    fn a_user_action_never_replaces_a_builtin() {
        let mut table = ScriptActions::default();
        let label = table.0[0].label.clone();
        table.upsert(ScriptAction::user(label, "(spawn-box 0 0 1 1)".into()));
        assert_eq!(table.builtins().count(), builtin_seed().len());
        assert_eq!(table.user().count(), 1);
        assert_ne!(table.0[0].source, "(spawn-box 0 0 1 1)");
    }
}
