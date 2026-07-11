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
    /// `(cut ax ay bx by width)` — sever bodies along a stroke.
    pub const CUT: &str = "cut";
    /// `(spawn-box x y w h)` — author a box body.
    pub const SPAWN_BOX: &str = "spawn-box";
    /// `(spawn-circle x y r)` — author a circle body.
    pub const SPAWN_CIRCLE: &str = "spawn-circle";
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
    /// `(sim-get path)` — read a simulation setting by reflect-path.
    pub const SIM_GET: &str = "sim-get";
    /// `(sim-set path value)` — write a simulation setting by reflect-path.
    pub const SIM_SET: &str = "sim-set";
    /// `(ops)` — list of every registered operation name.
    pub const OPS: &str = "ops";
    /// `(describe name)` — the signature and doc of one operation.
    pub const DESCRIBE: &str = "describe";
}

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
}

impl OpCategory {
    /// A short lower-case label for the reference panel.
    pub fn label(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Config => "config",
            Self::Query => "query",
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
    /// [`name`] constant.
    pub fn builtin() -> Self {
        use OpCategory::{Config, Edit, Query};
        Self {
            ops: vec![
                OpSpec {
                    name: name::SPAWN_BOX,
                    signature: "(spawn-box x y w h)",
                    doc: "Author a box body of size w×h centred at (x, y).",
                    category: Edit,
                    args: 4,
                },
                OpSpec {
                    name: name::SPAWN_CIRCLE,
                    signature: "(spawn-circle x y r)",
                    doc: "Author a circle body of radius r centred at (x, y).",
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
            ],
        }
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
