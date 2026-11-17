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
//! [`bridge`](crate::bridge) pairs each catalog entry with the steel
//! builtin that implements it, using the shared [`name`] constants so a name can
//! never drift between the catalog and the registration. The
//! [`OperationRegistry`](crate::bridge::OperationRegistry) resource is
//! this catalog surfaced into the ECS.

use gradiance_op::{OpRegistry, Seam};
use std::borrow::Cow;

/// Canonical operation names, shared by the catalog and the steel registration
/// so the two cannot drift. Referencing one `const` in both places makes a
/// mismatch a compile-time impossibility rather than a runtime bug.
pub mod name {
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
    /// `(camera-get path)` — read a camera-feel setting by reflect-path.
    pub const CAMERA_GET: &str = "camera-get";
    /// `(camera-set path value)` — write one (workstation config, not undoable).
    pub const CAMERA_SET: &str = "camera-set";
    /// `(register-action label source)` — surface a named action in the editor.
    pub const REGISTER_ACTION: &str = "register-action";
    /// `(action-x)` — x of the point the running action was invoked at.
    pub const ACTION_X: &str = "action-x";
    /// `(action-y)` — y of the point the running action was invoked at.
    pub const ACTION_Y: &str = "action-y";
    /// `(register-tool name points src)` — add a click-collecting tool to the
    /// palette.
    pub const REGISTER_TOOL: &str = "register-tool";
    /// `(tool-count)` — points the running tool collected.
    pub const TOOL_COUNT: &str = "tool-count";
    /// `(tool-x i)` — world x of the running tool's i-th point.
    pub const TOOL_X: &str = "tool-x";
    /// `(tool-y i)` — world y of the running tool's i-th point.
    pub const TOOL_Y: &str = "tool-y";
    /// `(touch-count i)` — how many bodies the i-th body is touching.
    pub const TOUCH_COUNT: &str = "touch-count";
    /// `(signal-set name value)` — publish a named value on the signal bus.
    pub const SIGNAL_SET: &str = "signal-set";
    /// `(signal-get name)` — the current value of a bus signal.
    pub const SIGNAL_GET: &str = "signal-get";
    /// `(body-friction i)` — read it back.
    pub const BODY_FRICTION: &str = "body-friction";
    /// `(body-restitution i)` — read it back.
    pub const BODY_RESTITUTION: &str = "body-restitution";
    /// `(body-density i)` — read it back.
    pub const BODY_DENSITY: &str = "body-density";
    /// `(body-static? i)` — whether the i-th body is static.
    pub const BODY_STATIC: &str = "body-static?";
    /// `(joint-count)` — how many joints exist.
    pub const JOINT_COUNT: &str = "joint-count";
    /// `(panel-show name)` — open an editor panel by name.
    pub const PANEL_SHOW: &str = "panel-show";
    /// `(panel-hide name)` — close an editor panel by name.
    pub const PANEL_HIDE: &str = "panel-hide";
    /// `(panel-toggle name)` — flip an editor panel's visibility.
    pub const PANEL_TOGGLE: &str = "panel-toggle";
    /// `(panel-open? name)` — whether a panel is currently shown.
    pub const PANEL_OPEN: &str = "panel-open?";
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
#[derive(Debug, Clone)]
pub struct OpSpec {
    /// The name the VM binds (from [`name`]).
    pub name: &'static str,
    /// A human-readable call form, e.g. `"(spawn-box x y w h)"`.
    pub signature: Cow<'static, str>,
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
    /// registered in [`bridge`](crate::bridge) under the same
    /// [`name`] constant. Grouped by seam so the list stays maintainable as
    /// verbs accrete.
    pub fn builtin() -> Self {
        let mut ops = Vec::new();
        ops.extend(query_specs());
        ops.extend(property_query_specs());
        ops.extend(action_specs());
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

impl OperationCatalog {
    /// The built-in catalog plus one row per registry operation
    /// (`gradiance-op`), projected from its declaration: the Lisp name, the
    /// signature from its declared fields, the doc and the seam.
    pub fn with_registry(registry: &OpRegistry) -> Self {
        let mut catalog = Self::builtin();
        catalog
            .ops
            .extend(registry.entries().iter().map(|entry| OpSpec {
                name: entry.meta.lisp,
                signature: Cow::Owned(entry.signature()),
                doc: entry.meta.doc,
                category: OpCategory::from(entry.meta.seam),
                args: entry.fields.len(),
            }));
        catalog
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

impl Default for OperationCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

/// Reads of a body's authored properties — the mirror image of the
/// `set-friction` … registry operations, so a script can inspect a value it
/// did not author.
fn property_query_specs() -> Vec<OpSpec> {
    use OpCategory::Query;
    vec![
        OpSpec {
            name: name::BODY_FRICTION,
            signature: Cow::Borrowed("(body-friction i)"),
            doc: "Coulomb friction of the i-th body.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_RESTITUTION,
            signature: Cow::Borrowed("(body-restitution i)"),
            doc: "Bounciness of the i-th body.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_DENSITY,
            signature: Cow::Borrowed("(body-density i)"),
            doc: "Mass density of the i-th body.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_STATIC,
            signature: Cow::Borrowed("(body-static? i)"),
            doc: "Whether the i-th body is static (1) or not (0).",
            category: Query,
            args: 1,
        },
    ]
}

/// Invocation-context reads: where the running action was invoked.
fn action_specs() -> Vec<OpSpec> {
    use OpCategory::Query;
    vec![
        OpSpec {
            name: name::ACTION_X,
            signature: Cow::Borrowed("(action-x)"),
            doc: "World x of the point the running action was invoked at (the click); NaN outside an action.",
            category: Query,
            args: 0,
        },
        OpSpec {
            name: name::ACTION_Y,
            signature: Cow::Borrowed("(action-y)"),
            doc: "World y of the point the running action was invoked at (the click); NaN outside an action.",
            category: Query,
            args: 0,
        },
        OpSpec {
            name: name::TOOL_COUNT,
            signature: Cow::Borrowed("(tool-count)"),
            doc: "How many points the running script tool collected; NaN outside a tool run.",
            category: Query,
            args: 0,
        },
        OpSpec {
            name: name::TOOL_X,
            signature: Cow::Borrowed("(tool-x i)"),
            doc: "World x of the running tool's i-th point (click order); NaN outside a tool run.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::TOOL_Y,
            signature: Cow::Borrowed("(tool-y i)"),
            doc: "World y of the running tool's i-th point (click order); NaN outside a tool run.",
            category: Query,
            args: 1,
        },
    ]
}

fn query_specs() -> Vec<OpSpec> {
    use OpCategory::Query;
    vec![
        OpSpec {
            name: name::PANEL_OPEN,
            signature: Cow::Borrowed("(panel-open? name)"),
            doc: "Whether an editor panel is currently shown (reads are total).",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::JOINT_COUNT,
            signature: Cow::Borrowed("(joint-count)"),
            doc: "Number of authored joints in the committed scene.",
            category: Query,
            args: 0,
        },
        OpSpec {
            name: name::BODY_X,
            signature: Cow::Borrowed("(body-x i)"),
            doc: "X of the i-th body (bodies ordered by id); NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_Y,
            signature: Cow::Borrowed("(body-y i)"),
            doc: "Y of the i-th body; NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_ROT,
            signature: Cow::Borrowed("(body-rot i)"),
            doc: "Rotation (radians) of the i-th body; NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::COUNT_AT,
            signature: Cow::Borrowed("(count-at x y)"),
            doc: "Number of bodies whose shape contains the world point (x, y).",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::NEAREST_AT,
            signature: Cow::Borrowed("(nearest-at x y)"),
            doc: "Index of the body whose centre is nearest (x, y); -1 if none.",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::NEAREST_DIST,
            signature: Cow::Borrowed("(nearest-dist x y)"),
            doc: "Distance from (x, y) to the nearest body centre; -1 if the scene is empty.",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::BODY_INDEX_AT,
            signature: Cow::Borrowed("(body-index-at x y)"),
            doc: "Index of a body whose shape contains (x, y) (first in id order); -1 if none.",
            category: Query,
            args: 2,
        },
        OpSpec {
            name: name::TOUCH_COUNT,
            signature: Cow::Borrowed("(touch-count i)"),
            doc: "How many bodies the i-th body is currently touching; NaN if out of range.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::SIGNAL_GET,
            signature: Cow::Borrowed("(signal-get name)"),
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
            signature: Cow::Borrowed("(sim-get path)"),
            doc: "Read a simulation setting by reflect-path, e.g. \"gravity.y\", \"speed\".",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::CAMERA_GET,
            signature: Cow::Borrowed("(camera-get path)"),
            doc: "Read a camera-feel setting: \"zoom_step\", \"pan_speed\", \"orbit_speed_deg\".",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::CAMERA_SET,
            signature: Cow::Borrowed("(camera-set path value)"),
            doc: "Set a camera-feel setting by reflect-path (workstation config, never undone).",
            category: Config,
            args: 2,
        },
    ]
}

/// The `CameraSettings` reflect paths `camera-get`/`camera-set` advertise;
/// the validation test resolves each.
pub const CAMERA_SETTING_PATHS: &[&str] = &["zoom_step", "pan_speed", "orbit_speed_deg"];

/// Editor-state verbs (write non-authored editor resources).
fn editor_specs() -> Vec<OpSpec> {
    vec![
        OpSpec {
            name: name::REGISTER_ACTION,
            signature: Cow::Borrowed("(register-action label source)"),
            doc: "Surface a named action (label + lisp source) in the editor's menus.",
            category: OpCategory::EditorState,
            args: 2,
        },
        OpSpec {
            name: name::REGISTER_TOOL,
            signature: Cow::Borrowed("(register-tool name points src)"),
            doc: "Add a palette tool that collects `points` clicks, then runs src once (read them with tool-x/tool-y).",
            category: OpCategory::EditorState,
            args: 3,
        },
        OpSpec {
            name: name::SIGNAL_SET,
            signature: Cow::Borrowed("(signal-set name value)"),
            doc: "Publish a named value on the signal bus (drives bindings; docs/signal-dataflow.md).",
            category: OpCategory::EditorState,
            args: 2,
        },
        OpSpec {
            name: name::LABEL,
            signature: Cow::Borrowed("(label body name)"),
            doc: "Name a body in the workspace (a viewport tag); body is a spawn's return value.",
            category: OpCategory::EditorState,
            args: 2,
        },
        OpSpec {
            name: name::PANEL_SHOW,
            signature: Cow::Borrowed("(panel-show name)"),
            doc: "Open an editor panel — the same toggle the View menu drives (try (panels)).",
            category: OpCategory::EditorState,
            args: 1,
        },
        OpSpec {
            name: name::PANEL_HIDE,
            signature: Cow::Borrowed("(panel-hide name)"),
            doc: "Close an editor panel by name.",
            category: OpCategory::EditorState,
            args: 1,
        },
        OpSpec {
            name: name::PANEL_TOGGLE,
            signature: Cow::Borrowed("(panel-toggle name)"),
            doc: "Flip an editor panel's visibility.",
            category: OpCategory::EditorState,
            args: 1,
        },
        OpSpec {
            name: name::DEFPARAM,
            signature: Cow::Borrowed("(defparam name value min max)"),
            doc: "Declare a tunable signal parameter (an auto-slider) published on the bus.",
            category: OpCategory::EditorState,
            args: 4,
        },
        OpSpec {
            name: name::DEFSIGNAL,
            signature: Cow::Borrowed("(defsignal name expr)"),
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
            signature: Cow::Borrowed("(ops)"),
            doc: "List of every registered operation name.",
            category: Query,
            args: 0,
        },
        OpSpec {
            name: name::DESCRIBE,
            signature: Cow::Borrowed("(describe name)"),
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
        assert_eq!(catalog.find(name::BODY_X).map(|o| o.args), Some(1));
        assert_eq!(
            catalog.find(name::JOINT_COUNT).map(|o| o.category),
            Some(OpCategory::Query)
        );
        assert!(catalog.find("no-such-op").is_none());
    }
}
