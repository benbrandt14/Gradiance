//! The operation **catalog**: the introspectable list of every Lisp verb, as
//! the console (completion, highlighting, the reference panel), the in-VM
//! `(ops)`/`(describe)` built-ins and the remote `gradiance.ops` method show
//! it.
//!
//! The catalog is a projection, not a declaration. The **operation
//! registry** is `gradiance-op`'s [`OpRegistry`]: every scene, query, config
//! and editor operation is declared once there, in its owning layer, and
//! [`OperationCatalog::with_registry`] projects one row per entry. The only
//! rows written here are the [`BUILTINS`]: the few verbs that exist only
//! inside a VM run, each with the reason it is not a registry operation.
//!
//! Pure data — no ECS, no `steel` — so it is unit-testable. The
//! [`OperationRegistry`](crate::bridge::OperationRegistry) resource is this
//! catalog surfaced into the ECS.

use gradiance_op::{OpRegistry, Seam};
use std::borrow::Cow;

/// The names of the [`BUILTINS`], shared by the catalog and the steel
/// registration so the two cannot drift.
pub mod name {
    /// `(action-x)` — x of the point the running action was invoked at.
    pub const ACTION_X: &str = "action-x";
    /// `(action-y)` — y of the point the running action was invoked at.
    pub const ACTION_Y: &str = "action-y";
    /// `(tool-count)` — points the running tool collected.
    pub const TOOL_COUNT: &str = "tool-count";
    /// `(tool-x i)` — world x of the running tool's i-th point.
    pub const TOOL_X: &str = "tool-x";
    /// `(tool-y i)` — world y of the running tool's i-th point.
    pub const TOOL_Y: &str = "tool-y";
    /// `(panel-show name)` — open an editor panel by name.
    pub const PANEL_SHOW: &str = "panel-show";
    /// `(panel-hide name)` — close an editor panel by name.
    pub const PANEL_HIDE: &str = "panel-hide";
    /// `(panel-toggle name)` — flip an editor panel's visibility.
    pub const PANEL_TOGGLE: &str = "panel-toggle";
    /// `(panel-open? name)` — whether a panel is currently shown.
    pub const PANEL_OPEN: &str = "panel-open?";
    /// `(ops)` — list of every operation name.
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

/// The `CameraSettings` reflect paths `camera-get`/`camera-set` advertise;
/// the validation test resolves each.
pub const CAMERA_SETTING_PATHS: &[&str] = &["zoom_step", "pan_speed", "orbit_speed_deg"];

/// Which sanctioned seam an operation routes through — the governance category
/// from `docs/script-lisp-decision.md` (reads total, writes seam-mediated).
/// The catalog's name for a registry operation's [`Seam`].
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

impl From<Seam> for OpCategory {
    fn from(seam: Seam) -> Self {
        match seam {
            Seam::Edit => Self::Edit,
            Seam::Config => Self::Config,
            Seam::Query => Self::Query,
            Seam::Editor => Self::EditorState,
        }
    }
}

/// A verb that exists only inside a VM run, not in the registry.
#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    /// The name the VM binds (from [`name`]).
    pub name: &'static str,
    /// The call form, e.g. `"(tool-x i)"`.
    pub signature: &'static str,
    /// One-line description.
    pub doc: &'static str,
    /// Which seam it routes through.
    pub category: OpCategory,
    /// How many arguments it takes.
    pub args: usize,
    /// Why it is a VM built-in and not a registry operation.
    pub why: &'static str,
}

/// Why the invocation-context reads are built-ins.
const RUN_CONTEXT: &str = "reads the running action's or tool's own invocation context, which exists only inside one VM run (a remote call or a menu row has none)";
/// Why the panel verbs are built-ins.
const PANELS: &str = "panel state lives in gradiance-ui, which registers `panel.*` operations in a later brief (cut from U-3)";
/// Why the introspection verbs are built-ins.
const META: &str =
    "reads the VM's own catalog; the remote surface has `gradiance.ops` for the same list";

/// Every VM built-in. Each has a steel builtin in
/// [`bridge`](crate::bridge) under the same [`name`] constant.
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: name::ACTION_X,
        signature: "(action-x)",
        doc: "World x of the point the running action was invoked at (the click); NaN outside an action.",
        category: OpCategory::Query,
        args: 0,
        why: RUN_CONTEXT,
    },
    Builtin {
        name: name::ACTION_Y,
        signature: "(action-y)",
        doc: "World y of the point the running action was invoked at (the click); NaN outside an action.",
        category: OpCategory::Query,
        args: 0,
        why: RUN_CONTEXT,
    },
    Builtin {
        name: name::TOOL_COUNT,
        signature: "(tool-count)",
        doc: "How many points the running script tool collected; NaN outside a tool run.",
        category: OpCategory::Query,
        args: 0,
        why: RUN_CONTEXT,
    },
    Builtin {
        name: name::TOOL_X,
        signature: "(tool-x i)",
        doc: "World x of the running tool's i-th point (click order); NaN outside a tool run.",
        category: OpCategory::Query,
        args: 1,
        why: RUN_CONTEXT,
    },
    Builtin {
        name: name::TOOL_Y,
        signature: "(tool-y i)",
        doc: "World y of the running tool's i-th point (click order); NaN outside a tool run.",
        category: OpCategory::Query,
        args: 1,
        why: RUN_CONTEXT,
    },
    Builtin {
        name: name::PANEL_SHOW,
        signature: "(panel-show name)",
        doc: "Open an editor panel — the same toggle the View menu drives (try (panels)).",
        category: OpCategory::EditorState,
        args: 1,
        why: PANELS,
    },
    Builtin {
        name: name::PANEL_HIDE,
        signature: "(panel-hide name)",
        doc: "Close an editor panel by name.",
        category: OpCategory::EditorState,
        args: 1,
        why: PANELS,
    },
    Builtin {
        name: name::PANEL_TOGGLE,
        signature: "(panel-toggle name)",
        doc: "Flip an editor panel's visibility.",
        category: OpCategory::EditorState,
        args: 1,
        why: PANELS,
    },
    Builtin {
        name: name::PANEL_OPEN,
        signature: "(panel-open? name)",
        doc: "Whether an editor panel is currently shown (reads are total).",
        category: OpCategory::Query,
        args: 1,
        why: PANELS,
    },
    Builtin {
        name: name::OPS,
        signature: "(ops)",
        doc: "List of every registered operation name.",
        category: OpCategory::Query,
        args: 0,
        why: META,
    },
    Builtin {
        name: name::DESCRIBE,
        signature: "(describe name)",
        doc: "The signature and doc string of one operation, as text.",
        category: OpCategory::Query,
        args: 1,
        why: META,
    },
];

/// One catalog row: a registry operation's projection or a [`Builtin`].
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// The Lisp name.
    pub name: &'static str,
    /// A human-readable call form, e.g. `"(spawn-box x y w h)"`.
    pub signature: Cow<'static, str>,
    /// One-line description for completion tooltips and the reference panel.
    pub doc: &'static str,
    /// Which seam it routes through.
    pub category: OpCategory,
    /// How many arguments it takes.
    pub args: usize,
}

impl From<&Builtin> for CatalogEntry {
    fn from(builtin: &Builtin) -> Self {
        Self {
            name: builtin.name,
            signature: Cow::Borrowed(builtin.signature),
            doc: builtin.doc,
            category: builtin.category,
            args: builtin.args,
        }
    }
}

/// The catalog of every verb the VM understands.
#[derive(Debug, Clone)]
pub struct OperationCatalog {
    ops: Vec<CatalogEntry>,
}

impl OperationCatalog {
    /// The [`BUILTINS`] alone (before the registry is projected).
    pub fn builtin() -> Self {
        Self {
            ops: BUILTINS.iter().map(CatalogEntry::from).collect(),
        }
    }

    /// The built-ins plus one row per registry operation, projected from its
    /// declaration: the Lisp name, the signature from its declared fields,
    /// the doc and the seam.
    pub fn with_registry(registry: &OpRegistry) -> Self {
        let mut catalog = Self::builtin();
        catalog
            .ops
            .extend(registry.entries().iter().map(|entry| CatalogEntry {
                name: entry.meta.lisp,
                signature: Cow::Owned(entry.signature()),
                doc: entry.meta.doc,
                category: OpCategory::from(entry.meta.seam),
                args: entry.fields.len(),
            }));
        catalog
    }

    /// Every entry, in catalog order (built-ins first).
    pub fn ops(&self) -> &[CatalogEntry] {
        &self.ops
    }

    /// Every operation name.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.ops.iter().map(|o| o.name)
    }

    /// The entry for `name`, if registered.
    pub fn find(&self, name: &str) -> Option<&CatalogEntry> {
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
    fn every_builtin_says_why_it_is_not_an_operation() {
        for builtin in BUILTINS {
            assert!(!builtin.why.is_empty(), "{}", builtin.name);
        }
    }

    #[test]
    fn find_resolves_known_and_rejects_unknown() {
        let catalog = OperationCatalog::builtin();
        assert_eq!(catalog.find(name::TOOL_X).map(|o| o.args), Some(1));
        assert_eq!(
            catalog.find(name::PANEL_SHOW).map(|o| o.category),
            Some(OpCategory::EditorState)
        );
        assert!(catalog.find("no-such-op").is_none());
    }
}
