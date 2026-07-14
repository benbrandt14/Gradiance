//! The operation catalog: the introspectable list of scene verbs the scripting
//! VM understands.
//!
//! Pure data — no ECS, no `steel` — so it is unit-testable and readable from any
//! layer. It is the **single source of truth** the console (syntax highlighting,
//! completion, the reference panel), the in-VM `(ops)`/`(describe)` builtins,
//! and — as the DSL grows into the tool's extension surface — data-driven menus
//! and user-authored tools all bind to. What the editor advertises therefore
//! always tracks what the VM actually implements.
//!
//! [`bridge`](crate::script::bridge) pairs each catalog entry with the steel
//! builtin that implements it, using the shared [`name`] constants so a name can
//! never drift between the catalog and the registration. The
//! [`OperationRegistry`](crate::script::bridge::OperationRegistry) resource is
//! this catalog surfaced into the ECS.

/// Canonical operation names, shared by the catalog and the steel registration
/// so the two cannot drift. Referencing one `const` in both places makes a
/// mismatch a compile-time impossibility rather than a runtime bug.
pub mod name {
    /// `(cut ax ay bx by width)` — re-exported from
    /// [`intent::name`](crate::command::intent::name) so verb, intent, and
    /// command share one greppable constant.
    pub use crate::command::intent::name::CUT;
    /// `(spawn-box x y w h)` — author a box body.
    pub const SPAWN_BOX: &str = "spawn-box";
    /// `(spawn-circle x y r)` — author a circle body.
    pub const SPAWN_CIRCLE: &str = "spawn-circle";
    /// `(spawn-ground x y angle)` — author a fixed ground half-plane.
    pub const SPAWN_GROUND: &str = "spawn-ground";
    /// `(body-count)` — how many bodies exist.
    pub const BODY_COUNT: &str = "body-count";
    /// `(body-x i)` — x of the i-th body (id order).
    pub const BODY_X: &str = "body-x";
    /// `(body-y i)` — y of the i-th body.
    pub const BODY_Y: &str = "body-y";
    /// `(body-rot i)` — rotation (radians) of the i-th body.
    pub const BODY_ROT: &str = "body-rot";
    /// `(count-at x y)` — bodies whose shape contains the point.
    pub const COUNT_AT: &str = "count-at";
    /// `(nearest-at x y)` — index of the body whose centre is nearest the point.
    pub const NEAREST_AT: &str = "nearest-at";
    /// `(nearest-dist x y)` — distance to the nearest body centre.
    pub const NEAREST_DIST: &str = "nearest-dist";
    /// `(body-index-at x y)` — index of a body whose shape contains the point.
    pub const BODY_INDEX_AT: &str = "body-index-at";
    /// `(sim-get path)` — read a simulation setting by reflect-path.
    pub const SIM_GET: &str = "sim-get";
    /// `(sim-set path value)` — write a simulation setting by reflect-path.
    pub const SIM_SET: &str = "sim-set";
    /// `(register-action label source)` — surface a named action in the editor.
    pub const REGISTER_ACTION: &str = "register-action";
    /// `(touch-count i)` — how many bodies the i-th body is touching.
    pub const TOUCH_COUNT: &str = "touch-count";
    /// `(signal-set name value)` — publish a named value on the signal bus.
    pub const SIGNAL_SET: &str = "signal-set";
    /// `(signal-get name)` — the current value of a bus signal.
    pub const SIGNAL_GET: &str = "signal-get";
    /// `(defparam name value min max)` — declare a tunable slider param.
    pub const DEFPARAM: &str = "defparam";
    /// `(defsignal name expr)` — declare a computed signal (RPN expression).
    pub const DEFSIGNAL: &str = "defsignal";
    /// `(label body name)` — name a body in the workspace (visible tag).
    pub const LABEL: &str = "label";
    /// `(ops)` — list of every registered operation name.
    pub const OPS: &str = "ops";
    /// `(describe name)` — the signature and doc of one operation.
    pub const DESCRIBE: &str = "describe";
}

/// The `SimSettings` reflect paths `sim-get`/`sim-set` advertise; the
/// validation test resolves each, so a field rename cannot strand one.
pub const SIM_SETTING_PATHS: &[&str] = &[
    "gravity.x",
    "gravity.y",
    "speed",
    "substeps",
    "plane_friction",
];

/// Which sanctioned seam an operation routes through — the governance category
/// from `docs/script-lisp-decision.md` (reads total, writes seam-mediated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCategory {
    /// Authored-world edit → emits an undoable intent (the command choke point).
    Edit,
    /// Editor configuration → writes a settings resource (the invariant-#4 seam).
    Config,
    /// Read-only geometric / physics query → returns a value, mutates nothing.
    Query,
    /// Editor-state change → writes a non-authored editor resource (e.g. the
    /// action table the UI surfaces). Not authored, not undoable, not physics.
    EditorState,
}

impl OpCategory {
    /// A short lower-case label for the reference panel.
    pub fn label(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Config => "config",
            Self::Query => "query",
            Self::EditorState => "editor",
        }
    }
}

/// One registered operation's metadata — a catalog entry.
#[derive(Debug, Clone, Copy)]
pub struct OpSpec {
    /// The name the VM binds (from [`name`]).
    pub name: &'static str,
    /// A human-readable call form, e.g. `"(spawn-box x y w h)"`.
    pub signature: &'static str,
    /// One-line description for completion tooltips and the reference panel.
    pub doc: &'static str,
    /// Which seam it routes through.
    pub category: OpCategory,
    /// How many arguments it takes (for future arity checks and tooling).
    pub args: usize,
}

/// The catalog of every operation the VM understands.
#[derive(Debug, Clone)]
pub struct OperationCatalog {
    ops: Vec<OpSpec>,
}

impl OperationCatalog {
    /// The canonical built-in catalog. Each entry has a matching steel builtin
    /// registered in [`bridge`](crate::script::bridge) under the same
    /// [`name`] constant. Grouped by seam so the list stays maintainable as
    /// verbs accrete.
    pub fn builtin() -> Self {
        let mut ops = Vec::new();
        ops.extend(edit_specs());
        ops.extend(query_specs());
        ops.extend(config_specs());
        ops.extend(editor_specs());
        ops.extend(meta_specs());
        Self { ops }
    }

    /// Every entry, in catalog order (edits first, then queries).
    pub fn ops(&self) -> &[OpSpec] {
        &self.ops
    }

    /// Every operation name.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.ops.iter().map(|o| o.name)
    }

    /// The entry for `name`, if registered.
    pub fn find(&self, name: &str) -> Option<&OpSpec> {
        self.ops.iter().find(|o| o.name == name)
    }
}

impl Default for OperationCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

/// Authored-world edit verbs (→ intents).
fn edit_specs() -> Vec<OpSpec> {
    use OpCategory::Edit;
    vec![
        OpSpec {
            name: name::SPAWN_BOX,
            signature: "(spawn-box x y w h)",
            doc: "Author a box body of size w×h centred at (x, y); returns its handle.",
            category: Edit,
            args: 4,
        },
        OpSpec {
            name: name::SPAWN_CIRCLE,
            signature: "(spawn-circle x y r)",
            doc: "Author a circle body of radius r centred at (x, y); returns its handle.",
            category: Edit,
            args: 3,
        },
        OpSpec {
            name: name::SPAWN_GROUND,
            signature: "(spawn-ground x y angle)",
            doc: "Author a fixed ground half-plane through (x, y), tilted by angle radians.",
            category: Edit,
            args: 3,
        },
        OpSpec {
            name: name::CUT,
            signature: "(cut ax ay bx by width)",
            doc: "Sever every body crossed by the stroke a→b of the given width.",
            category: Edit,
            args: 5,
        },
    ]
}

/// Read-total geometric query verbs (→ the scene snapshot).
fn query_specs() -> Vec<OpSpec> {
    use OpCategory::Query;
    vec![
        OpSpec {
            name: name::BODY_COUNT,
            signature: "(body-count)",
            doc: "Number of authored bodies in the committed scene.",
            category: Query,
            args: 0,
        },
        OpSpec {
            name: name::BODY_X,
            signature: "(body-x i)",
            doc: "X of the i-th body (bodies ordered by id); NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_Y,
            signature: "(body-y i)",
            doc: "Y of the i-th body; NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_ROT,
            signature: "(body-rot i)",
            doc: "Rotation (radians) of the i-th body; NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::COUNT_AT,
            signature: "(count-at x y)",
            doc: "Number of bodies whose shape contains the world point (x, y).",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::NEAREST_AT,
            signature: "(nearest-at x y)",
            doc: "Index of the body whose centre is nearest (x, y); -1 if none.",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::NEAREST_DIST,
            signature: "(nearest-dist x y)",
            doc: "Distance from (x, y) to the nearest body centre; -1 if the scene is empty.",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::BODY_INDEX_AT,
            signature: "(body-index-at x y)",
            doc: "Index of a body whose shape contains (x, y) (first in id order); -1 if none.",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::TOUCH_COUNT,
            signature: "(touch-count i)",
            doc: "How many bodies the i-th body is currently touching; NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::SIGNAL_GET,
            signature: "(signal-get name)",
            doc: "Current value of a named signal on the bus; NaN if unset.",
            category: Query,
            args: 1,
        },
    ]
}

/// Editor-configuration verbs (`sim-get` reads, `sim-set` writes the settings
/// resource).
fn config_specs() -> Vec<OpSpec> {
    use OpCategory::{Config, Query};
    vec![
        OpSpec {
            name: name::SIM_GET,
            signature: "(sim-get path)",
            doc: "Read a simulation setting by reflect-path, e.g. \"gravity.y\", \"speed\".",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::SIM_SET,
            signature: "(sim-set path value)",
            doc: "Set a simulation setting by reflect-path (config seam, not undoable).",
            category: Config,
            args: 2,
        },
    ]
}

/// Editor-state verbs (write non-authored editor resources).
fn editor_specs() -> Vec<OpSpec> {
    vec![
        OpSpec {
            name: name::REGISTER_ACTION,
            signature: "(register-action label source)",
            doc: "Surface a named action (label + lisp source) in the editor's menus.",
            category: OpCategory::EditorState,
            args: 2,
        },
        OpSpec {
            name: name::SIGNAL_SET,
            signature: "(signal-set name value)",
            doc: "Publish a named value on the signal bus (drives bindings; docs/signal-dataflow.md).",
            category: OpCategory::EditorState,
            args: 2,
        },
        OpSpec {
            name: name::LABEL,
            signature: "(label body name)",
            doc: "Name a body in the workspace (a viewport tag); body is a spawn's return value.",
            category: OpCategory::EditorState,
            args: 2,
        },
        OpSpec {
            name: name::DEFPARAM,
            signature: "(defparam name value min max)",
            doc: "Declare a tunable signal parameter (an auto-slider) published on the bus.",
            category: OpCategory::EditorState,
            args: 4,
        },
        OpSpec {
            name: name::DEFSIGNAL,
            signature: "(defsignal name expr)",
            doc: "Declare a computed signal from an RPN expression over other signals (e.g. \"t sin amp *\").",
            category: OpCategory::EditorState,
            args: 2,
        },
    ]
}

/// Homoiconic introspection verbs (the catalog reading itself).
fn meta_specs() -> Vec<OpSpec> {
    use OpCategory::Query;
    vec![
        OpSpec {
            name: name::OPS,
            signature: "(ops)",
            doc: "List of every registered operation name.",
            category: Query,
            args: 0,
        },
        OpSpec {
            name: name::DESCRIBE,
            signature: "(describe name)",
            doc: "The signature and doc string of one operation, as text.",
            category: Query,
            args: 1,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_names_are_unique() {
        let catalog = OperationCatalog::builtin();
        let mut names: Vec<&str> = catalog.names().collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            count,
            "duplicate operation name in the catalog"
        );
    }

    #[test]
    fn every_entry_signature_starts_with_its_name() {
        // A cheap guard that a copy-paste didn't leave a signature describing a
        // different verb than its `name`.
        for op in OperationCatalog::builtin().ops() {
            let head = format!("({}", op.name);
            assert!(
                op.signature.starts_with(&head),
                "signature `{}` does not open with `{head}`",
                op.signature,
            );
        }
    }

    #[test]
    fn find_resolves_known_and_rejects_unknown() {
        let catalog = OperationCatalog::builtin();
        assert_eq!(catalog.find(name::CUT).map(|o| o.args), Some(5));
        assert_eq!(
            catalog.find(name::BODY_COUNT).map(|o| o.category),
            Some(OpCategory::Query)
        );
        assert!(catalog.find("no-such-op").is_none());
    }
}
