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

/// Canonical operation names, shared by the catalog and the steel registration
/// so the two cannot drift. Referencing one `const` in both places makes a
/// mismatch a compile-time impossibility rather than a runtime bug.
pub mod name {
    /// `(cut ax ay bx by width)` — re-exported from
    /// [`intent::name`](gradiance_command::intent::name) so verb, intent, and
    /// command share one greppable constant.
    pub use gradiance_command::intent::name::CUT;
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
    /// `(set-friction i v)` — Coulomb friction of the i-th body.
    pub const SET_FRICTION: &str = "set-friction";
    /// `(set-restitution i v)` — bounciness of the i-th body.
    pub const SET_RESTITUTION: &str = "set-restitution";
    /// `(set-density i v)` — mass density of the i-th body.
    pub const SET_DENSITY: &str = "set-density";
    /// `(set-static i on)` — make the i-th body static (or dynamic again).
    pub const SET_STATIC: &str = "set-static";
    /// `(body-friction i)` — read it back.
    pub const BODY_FRICTION: &str = "body-friction";
    /// `(body-restitution i)` — read it back.
    pub const BODY_RESTITUTION: &str = "body-restitution";
    /// `(body-density i)` — read it back.
    pub const BODY_DENSITY: &str = "body-density";
    /// `(body-static? i)` — whether the i-th body is static.
    pub const BODY_STATIC: &str = "body-static?";
    /// `(place i x y angle)` — move/rotate the i-th body.
    pub const PLACE: &str = "place";
    /// `(hinge a b x y)` — pin two bodies at a world point.
    pub const HINGE: &str = "hinge";
    /// `(slider a b x y ax ay)` — prismatic joint along an axis.
    pub const SLIDER: &str = "slider";
    /// `(spring a b stiffness damping)` — spring-damper strut.
    pub const SPRING: &str = "spring";
    /// `(joint-count)` — how many joints exist.
    pub const JOINT_COUNT: &str = "joint-count";
    /// `(undo)` — undo the last command.
    pub const UNDO: &str = "undo";
    /// `(redo)` — redo the last undone command.
    pub const REDO: &str = "redo";
    /// `(delete i)` — delete the i-th body (id order).
    pub const DELETE: &str = "delete";
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
    /// registered in [`bridge`](crate::bridge) under the same
    /// [`name`] constant. Grouped by seam so the list stays maintainable as
    /// verbs accrete.
    pub fn builtin() -> Self {
        let mut ops = Vec::new();
        ops.extend(spawn_specs());
        ops.extend(mutate_specs());
        ops.extend(property_specs());
        ops.extend(query_specs());
        ops.extend(property_query_specs());
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
/// Edits that **author new geometry** — the spawn family and the cut.
fn spawn_specs() -> Vec<OpSpec> {
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
    ]
}

/// Edits over **existing bodies**: their pose, their properties, their
/// relationships, and the history ops. Split from [`spawn_specs`] to mirror the
/// bridge's own split — these all name their target by index.
fn mutate_specs() -> Vec<OpSpec> {
    use OpCategory::Edit;
    vec![
        OpSpec {
            name: name::PLACE,
            signature: "(place i x y angle)",
            doc: "Move the i-th body to (x, y) and rotate it to `angle` radians — one undo step.",
            category: OpCategory::Edit,
            args: 4,
        },
        OpSpec {
            name: name::HINGE,
            signature: "(hinge a b x y)",
            doc: "Hinge bodies a and b at world point (x, y); b < 0 pins a to the world.",
            category: OpCategory::Edit,
            args: 4,
        },
        OpSpec {
            name: name::SLIDER,
            signature: "(slider a b x y ax ay)",
            doc: "Prismatic joint at (x, y) sliding along world axis (ax, ay); b < 0 pins to the world.",
            category: OpCategory::Edit,
            args: 6,
        },
        OpSpec {
            name: name::SPRING,
            signature: "(spring a b stiffness damping)",
            doc: "Spring-damper strut between the centres of a and b; rest length is their current distance.",
            category: OpCategory::Edit,
            args: 4,
        },
        OpSpec {
            name: name::DELETE,
            signature: "(delete i)",
            doc: "Delete the i-th body (id order, same index as body-x) — undoable.",
            category: OpCategory::Edit,
            args: 1,
        },
        OpSpec {
            name: name::UNDO,
            signature: "(undo)",
            doc: "Undo the last command — the same step Edit ▸ Undo takes.",
            category: OpCategory::Edit,
            args: 0,
        },
        OpSpec {
            name: name::REDO,
            signature: "(redo)",
            doc: "Redo the last undone command.",
            category: OpCategory::Edit,
            args: 0,
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

/// Body-property edits — the inspector's fields as ops.
fn property_specs() -> Vec<OpSpec> {
    vec![
        OpSpec {
            name: name::SET_FRICTION,
            signature: "(set-friction i v)",
            doc: "Set the i-th body's Coulomb friction (static and dynamic) — undoable.",
            category: OpCategory::Edit,
            args: 2,
        },
        OpSpec {
            name: name::SET_RESTITUTION,
            signature: "(set-restitution i v)",
            doc: "Set the i-th body's bounciness, 0 = dead, 1 = perfectly elastic — undoable.",
            category: OpCategory::Edit,
            args: 2,
        },
        OpSpec {
            name: name::SET_DENSITY,
            signature: "(set-density i v)",
            doc: "Set the i-th body's mass density (area x density = mass) — undoable.",
            category: OpCategory::Edit,
            args: 2,
        },
        OpSpec {
            name: name::SET_STATIC,
            signature: "(set-static i on)",
            doc: "Make the i-th body static when `on` is non-zero, dynamic otherwise — undoable.",
            category: OpCategory::Edit,
            args: 2,
        },
    ]
}

/// Reads of a body's authored properties — the mirror image of
/// [`property_specs`], so a script can inspect a value it did not author.
fn property_query_specs() -> Vec<OpSpec> {
    use OpCategory::Query;
    vec![
        OpSpec {
            name: name::BODY_FRICTION,
            signature: "(body-friction i)",
            doc: "Coulomb friction of the i-th body.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_RESTITUTION,
            signature: "(body-restitution i)",
            doc: "Bounciness of the i-th body.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_DENSITY,
            signature: "(body-density i)",
            doc: "Mass density of the i-th body.",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::BODY_STATIC,
            signature: "(body-static? i)",
            doc: "Whether the i-th body is static (1) or not (0).",
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
            signature: "(panel-open? name)",
            doc: "Whether an editor panel is currently shown (reads are total).",
            category: Query,
            args: 1,
        },
        OpSpec {
            name: name::JOINT_COUNT,
            signature: "(joint-count)",
            doc: "Number of authored joints in the committed scene.",
            category: Query,
            args: 0,
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
            name: name::PANEL_SHOW,
            signature: "(panel-show name)",
            doc: "Open an editor panel — the same toggle the View menu drives (try (panels)).",
            category: OpCategory::EditorState,
            args: 1,
        },
        OpSpec {
            name: name::PANEL_HIDE,
            signature: "(panel-hide name)",
            doc: "Close an editor panel by name.",
            category: OpCategory::EditorState,
            args: 1,
        },
        OpSpec {
            name: name::PANEL_TOGGLE,
            signature: "(panel-toggle name)",
            doc: "Flip an editor panel's visibility.",
            category: OpCategory::EditorState,
            args: 1,
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
