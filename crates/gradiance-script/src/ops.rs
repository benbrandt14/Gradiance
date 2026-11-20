//! The operations this layer owns (`gradiance-op`): the **editor** verbs
//! that extend the tool (`register-action`, `register-tool`, `label`), and
//! the script side of the signal bus (`signal-get`, `signal-set`,
//! `defparam`, `defsignal`), and the signal graph's edit surface
//! (`graph-add`, `graph-connect`, `graph-set`, `graph-remove`; brief G-2).
//!
//! They live here because the tables they fill do: the action table and the
//! workspace labels are this crate's, and this is the lowest layer that sees
//! both the signal bus and the registry (`gradiance-signal` has no edge to
//! `gradiance-op`).
//!
//! An operation cannot write a resource directly ([`OpOutput`] holds only
//! messages and reflect-path settings writes), so each lowers to one
//! [`EditorRequest`] message and [`apply_editor_requests`] moves it into its
//! table. In a script run the message waits in the run's queue like an edit
//! intent, so a failed run discards it too (all or nothing).

use crate::actions::{ScriptAction, ScriptActions};
use crate::bridge::WorkspaceLabels;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::script_tools::{ScriptToolSpec, ScriptTools};
use gradiance_domain::signal::SignalExpr;
use gradiance_graph::{Family, GraphEdit, Input, NodeKind, NodeRef, PortRef};
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Value,
    json,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Most points one script tool may collect: a tool that asks for more is a
/// typo, not a gesture anyone will finish.
pub const MAX_TOOL_POINTS: usize = 64;

/// One editor-state change an operation asks for, applied by
/// [`apply_editor_requests`].
#[derive(Message, Debug, Clone)]
pub enum EditorRequest {
    /// Add (or replace, by label) a user action.
    Action(ScriptAction),
    /// Add (or replace, by name) a palette tool.
    Tool(ScriptToolSpec),
    /// Name an object in the workspace.
    Label {
        /// The display name.
        name: String,
        /// The object it names.
        id: StableId,
    },
    /// Publish a value on the signal bus.
    Publish {
        /// The signal's name.
        name: String,
        /// Its value.
        value: f64,
    },
    /// Edit the signal graph (`defparam`, `defsignal`, `defkey` and the
    /// `graph.*` operations). Names resolve when the edit is applied, so a
    /// node declared earlier in the same run is found.
    Graph(GraphEdit),
    /// Write recorded signal history as CSV (`signal.export`, brief R-1).
    Export {
        /// The file.
        path: PathBuf,
        /// The CSV text, built from the run's snapshot.
        csv: String,
    },
}

/// The signal bus's current values, as a read-only context mirror
/// (`signal-get`). One mirror per context, so one run reads one snapshot.
#[derive(Debug, Clone, Default)]
pub struct SignalValues(pub HashMap<String, f64>);

/// The signal bus's recorded histories, as a read-only context mirror
/// (`signal-history`, `signal-export`): each signal's `(t, value)` samples,
/// oldest first, `t` in simulated seconds.
#[derive(Debug, Clone, Default)]
pub struct SignalHistories(pub HashMap<String, Vec<(f32, f32)>>);

/// Registers this layer's operations, the signal mirror, the request
/// message and the system that applies it.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<RegisterAction>();
        registry.register::<RegisterTool>();
        registry.register::<Label>();
        registry.register::<SignalGet>();
        registry.register::<SignalSet>();
        registry.register::<DefParam>();
        registry.register::<DefSignal>();
        registry.register::<DefKey>();
        registry.register::<GraphAdd>();
        registry.register::<GraphConnect>();
        registry.register::<GraphSet>();
        registry.register::<GraphRemove>();
        registry.register::<SignalHistory>();
        registry.register::<SignalExport>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_signal_mirror);
    app.add_message::<EditorRequest>();
}

/// Publishes the signal bus's values into a new operation context.
fn fill_signal_mirror(world: &mut World, ctx: &mut OpContext) {
    if let Some(bus) = world.get_resource::<gradiance_signal::SignalBus>() {
        ctx.insert(SignalValues(
            bus.entries()
                .map(|(name, entry)| (name.to_owned(), f64::from(entry.value())))
                .collect(),
        ));
        ctx.insert(SignalHistories(
            bus.entries()
                .map(|(name, entry)| {
                    let samples = entry
                        .times()
                        .iter()
                        .copied()
                        .zip(entry.history().iter().copied())
                        .collect();
                    (name.to_owned(), samples)
                })
                .collect(),
        ));
    }
}

/// The metadata of an editor operation: no menu, no chord, no undo step.
const fn editor(
    id: &'static str,
    lisp: &'static str,
    doc: &'static str,
    example: &'static str,
) -> OpMeta {
    OpMeta {
        id,
        lisp,
        doc,
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example,
    }
}

/// Surface a named action in the editor's menus.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RegisterAction {
    /// The label the menu shows (unique: re-registering replaces).
    pub label: String,
    /// The Lisp source the action runs.
    pub source: String,
}

impl Operation for RegisterAction {
    const META: OpMeta = editor(
        "action.register",
        "register-action",
        "Surface a named action (label + lisp source) in the editor's menus.",
        r#"{"label": "Fill", "source": "(spawn-box 0 0 1 1)"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(
            OpOutput::none().message(EditorRequest::Action(ScriptAction::user(
                self.label,
                self.source,
            ))),
        )
    }
}

/// Add a click-collecting tool to the palette.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RegisterTool {
    /// The palette name (unique: re-registering replaces).
    pub name: String,
    /// How many clicks the tool collects, 1 to [`MAX_TOOL_POINTS`].
    pub points: f64,
    /// The Lisp source that runs once the points are in.
    pub src: String,
}

impl Operation for RegisterTool {
    const META: OpMeta = editor(
        "tool.register",
        "register-tool",
        "Add a palette tool that collects `points` clicks, then runs src once (read them with tool-x/tool-y).",
        r#"{"name": "peg", "points": 1, "src": "(spawn-circle (tool-x 0) (tool-y 0) 0.2)"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let name = self.name.trim().to_owned();
        if name.is_empty() {
            return Err(OpError::Invalid("the name is empty".to_owned()));
        }
        let n = self.points;
        if n.fract() != 0.0 || !(1.0..=MAX_TOOL_POINTS as f64).contains(&n) {
            return Err(OpError::Invalid(format!(
                "points must be a whole number from 1 to {MAX_TOOL_POINTS}, got {n}"
            )));
        }
        Ok(
            OpOutput::none().message(EditorRequest::Tool(ScriptToolSpec {
                name,
                points: n as usize,
                source: self.src,
            })),
        )
    }
}

/// Name an object in the workspace (a viewport tag).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Label {
    /// The object's handle: a spawn's return value (its id).
    pub body: String,
    /// The name to show.
    pub name: String,
}

impl Operation for Label {
    const META: OpMeta = editor(
        "workspace.label",
        "label",
        "Name a body in the workspace (a viewport tag); body is a spawn's return value.",
        r#"{"body": "00000000-0000-0000-0000-00000000000a", "name": "crate"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        // A handle, not a `Target`: a script labels the body it spawned in
        // the same run, which the run's snapshot does not hold yet.
        let id = uuid::Uuid::parse_str(self.body.trim())
            .map_err(|_| OpError::Invalid(format!("`{}` is not a body handle", self.body)))?;
        Ok(OpOutput::none().message(EditorRequest::Label {
            name: self.name,
            id: StableId(id),
        }))
    }
}

/// Read a bus signal.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SignalGet {
    /// The signal's name.
    pub name: String,
}

impl Operation for SignalGet {
    const META: OpMeta = OpMeta {
        id: "signal.get",
        lisp: "signal-get",
        doc: "Current value of a named signal on the bus; NaN if unset.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "t"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let values = ctx.get::<SignalValues>()?;
        let value = values.0.get(&self.name).copied().unwrap_or(f64::NAN);
        Ok(OpOutput::none().returning(value))
    }
}

/// Publish a value on the signal bus.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SignalSet {
    /// The signal's name.
    pub name: String,
    /// Its value.
    pub value: f64,
}

impl Operation for SignalSet {
    const META: OpMeta = editor(
        "signal.set",
        "signal-set",
        "Publish a named value on the signal bus (drives bindings; docs/signal-dataflow.md).",
        r#"{"name": "excitement", "value": 7}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(EditorRequest::Publish {
            name: self.name,
            value: self.value,
        }))
    }
}

/// Declare a tunable parameter: a param node of the signal graph. The graph
/// is scene content (saved with the scene), so a declaration records one
/// settled `settings` undo step through `commit_settings_edits` (brief
/// U-2c). Re-declaring a name updates that node, so its wires hold.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct DefParam {
    /// The parameter's name (unique: re-declaring updates it).
    pub name: String,
    /// Its value.
    pub value: f64,
    /// The slider's lower end.
    pub min: f64,
    /// The slider's upper end.
    pub max: f64,
}

impl Operation for DefParam {
    const META: OpMeta = OpMeta {
        id: "signal.param",
        lisp: "defparam",
        doc: "Declare a tunable signal parameter (an auto-slider) published on the bus.",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(gradiance_command::intent::name::SETTINGS),
        example: r#"{"name": "amp", "value": 2, "min": 0, "max": 10}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(
            OpOutput::none().message(EditorRequest::Graph(GraphEdit::Param {
                id: ctx.new_id(),
                name: self.name,
                value: self.value as f32,
                min: self.min as f32,
                max: self.max as f32,
            })),
        )
    }
}

/// Declare a computed signal from an RPN expression: an expression node of
/// the signal graph whose names are wired, by id, to the nodes labelled
/// with them (a name no node has stays a bus read). Scene content with one
/// settled `settings` undo step, like [`DefParam`].
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct DefSignal {
    /// The signal's name (unique: re-declaring updates it).
    pub name: String,
    /// The RPN expression over other signals and `t`.
    pub expr: String,
}

impl Operation for DefSignal {
    const META: OpMeta = OpMeta {
        id: "signal.computed",
        lisp: "defsignal",
        doc: "Declare a computed signal from an RPN expression over other signals (e.g. \"t sin amp *\").",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(gradiance_command::intent::name::SETTINGS),
        example: r#"{"name": "osc", "expr": "t sin 2 *"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let expr = SignalExpr::parse_rpn(&self.expr)
            .map_err(|err| OpError::Invalid(format!("`{}`: {err}", self.expr)))?;
        Ok(
            OpOutput::none().message(EditorRequest::Graph(GraphEdit::Expr {
                id: ctx.new_id(),
                name: self.name,
                expr,
            })),
        )
    }
}

/// Declare a key input (brief N-2): a key node of the signal graph. Scene
/// content with one settled `settings` undo step, like [`DefSignal`].
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct DefKey {
    /// The input's name (unique: re-declaring updates it).
    pub name: String,
    /// The key that gives `+1` while held in play (`E`, `Left`, `Space`).
    pub key: String,
    /// The key that gives `-1` (absent: a single key, `0`/`1`).
    pub negative: Option<String>,
}

impl Operation for DefKey {
    const META: OpMeta = OpMeta {
        id: "signal.key",
        lisp: "defkey",
        doc: "Declare a key input: a signal that is 1 while key is held in play (0 paused or while a field has the keyboard); with negative, -1 while that key is held and 0 when both are. While the sim plays the key is the input's, not the editor's.",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(gradiance_command::intent::name::SETTINGS),
        example: r#"{"name": "throttle", "key": "E", "negative": "Q"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let canonical = |name: &str| {
            gradiance_domain::keys::named_key(name)
                .map(|k| k.name.to_owned())
                .ok_or_else(|| OpError::Invalid(format!("`{name}` is not a key name")))
        };
        let key = canonical(&self.key)?;
        let negative = self.negative.as_deref().map(canonical).transpose()?;
        Ok(
            OpOutput::none().message(EditorRequest::Graph(GraphEdit::Key {
                id: ctx.new_id(),
                name: self.name,
                key,
                negative,
            })),
        )
    }
}

/// The metadata of a `graph.*` operation: config seam, one settled
/// `settings` undo step (the graph is scene content), no menu, no chord.
const fn graph_op(
    id: &'static str,
    lisp: &'static str,
    doc: &'static str,
    example: &'static str,
) -> OpMeta {
    OpMeta {
        id,
        lisp,
        doc,
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: Some(gradiance_command::intent::name::SETTINGS),
        example,
    }
}

/// Add a node to the signal graph.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphAdd {
    /// The catalog id of its kind (`signal::gain`; `docs/nodes.md`).
    pub kind: String,
    /// The bus name it publishes under (absent: it publishes nothing and
    /// a wire from it inlines its value).
    pub label: Option<String>,
}

impl Operation for GraphAdd {
    const META: OpMeta = graph_op(
        "graph.add",
        "graph-add",
        "Add a signal node of a catalog kind (docs/nodes.md), optionally labelled; returns its id. Its ports read their defaults until set or wired. Placed below the last node of its canvas column.",
        r#"{"kind": "signal::gain", "label": "boost"}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let kind = NodeKind::from_catalog_id(self.kind.trim()).ok_or_else(|| {
            OpError::Invalid(format!(
                "`{}` is not a node kind a call can add (docs/nodes.md; an expression is a defsignal)",
                self.kind
            ))
        })?;
        // Shape nodes have no consumer until a body can name one (G-4).
        if kind.family() != Family::Signal {
            return Err(OpError::Invalid(format!(
                "`{}` is a shape node: shapes join the graph with brief G-4",
                self.kind
            )));
        }
        let id = ctx.new_id();
        Ok(OpOutput::none()
            .message(EditorRequest::Graph(GraphEdit::Add {
                id,
                kind,
                label: self.label,
                inputs: Vec::new(),
            }))
            .returning(id.to_string()))
    }
}

/// Wire one node's output into another's input.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphConnect {
    /// The source node: its id or its label.
    pub from: String,
    /// The node whose input is wired: its id or its label.
    pub to: String,
    /// The input port's name (`in`, `k`; an expression's port is the bus
    /// name it reads).
    pub port: String,
}

impl Operation for GraphConnect {
    const META: OpMeta = graph_op(
        "graph.connect",
        "graph-connect",
        "Wire node from's output into input port of node to (nodes by id or label). The wire is by id: renaming either node keeps it.",
        r#"{"from": "level", "to": "wave", "port": "t"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(
            OpOutput::none().message(EditorRequest::Graph(GraphEdit::Connect {
                from: NodeRef::parse(&self.from),
                to: NodeRef::parse(&self.to),
                port: PortRef::Name(self.port),
            })),
        )
    }
}

/// What `graph-set` writes: a number, a point or a name.
#[derive(Debug, Clone, PartialEq, Deserialize, Reflect)]
#[serde(try_from = "GraphValueRepr")]
pub enum GraphValue {
    /// A scalar.
    Number(f64),
    /// A point `[x, y]`.
    Point(Vec2),
    /// A name.
    Text(String),
}

/// The JSON shapes a [`GraphValue`] decodes from.
#[derive(Deserialize)]
#[serde(untagged)]
enum GraphValueRepr {
    Number(f64),
    Point([f32; 2]),
    Text(String),
}

impl TryFrom<GraphValueRepr> for GraphValue {
    type Error = String;

    fn try_from(repr: GraphValueRepr) -> Result<Self, String> {
        Ok(match repr {
            GraphValueRepr::Number(n) if n.is_finite() => Self::Number(n),
            GraphValueRepr::Number(n) => return Err(format!("{n} is not a finite number")),
            GraphValueRepr::Point([x, y]) => Self::Point(Vec2::new(x, y)),
            GraphValueRepr::Text(text) => Self::Text(text),
        })
    }
}

/// Set a port constant, a label or a canvas position.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphSet {
    /// The node: its id or its label.
    pub node: String,
    /// `"label"`, `"position"`, or an input port's name.
    pub key: String,
    /// A label is a name (`""` clears it); a position is `[x, y]`; a port
    /// takes a number or a point (a constant, which disconnects it) or a
    /// name (a bus read).
    pub value: GraphValue,
}

impl Operation for GraphSet {
    const META: OpMeta = graph_op(
        "graph.set",
        "graph-set",
        "Set a node's \"label\" (a name; \"\" stops it publishing), its \"position\" on the canvas ([x y]), or an input port: a number or point is a constant (it disconnects the port), a name reads that bus signal.",
        r#"{"node": "wave", "key": "position", "value": [300, 40]}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        let node = NodeRef::parse(&self.node);
        let edit = match (self.key.as_str(), self.value) {
            ("label", GraphValue::Text(text)) => GraphEdit::SetLabel {
                node,
                label: (!text.trim().is_empty()).then_some(text),
            },
            ("position", GraphValue::Point(pos)) => GraphEdit::SetPos { node, pos },
            (key @ ("label" | "position"), value) => {
                return Err(OpError::Invalid(format!(
                    "a {key} takes {}, not {value:?}",
                    if key == "label" { "a name" } else { "[x y]" }
                )));
            }
            (port, value) => GraphEdit::SetInput {
                node,
                port: PortRef::Name(port.to_owned()),
                input: match value {
                    GraphValue::Number(n) => Input::scalar(n as f32),
                    GraphValue::Point(p) => Input::Const(gradiance_graph::Value::Vec2(p)),
                    GraphValue::Text(name) => Input::External(name),
                },
            },
        };
        Ok(OpOutput::none().message(EditorRequest::Graph(edit)))
    }
}

/// Remove a node from the signal graph.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphRemove {
    /// The node: its id or its label.
    pub node: String,
}

impl Operation for GraphRemove {
    const META: OpMeta = graph_op(
        "graph.remove",
        "graph-remove",
        "Remove a node (by id or label). A wire from it becomes a bus read of its label (or its port's default if it had none).",
        r#"{"node": "wave"}"#,
    );

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(
            OpOutput::none().message(EditorRequest::Graph(GraphEdit::Remove {
                node: NodeRef::parse(&self.node),
            })),
        )
    }
}

/// A signal's recorded samples as `[t, v]` pairs.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SignalHistory {
    /// The signal's name.
    pub name: String,
}

impl Operation for SignalHistory {
    const META: OpMeta = OpMeta {
        id: "signal.history",
        lisp: "signal-history",
        doc: "A signal's recorded samples, oldest first, as a list of [t v] (t in simulated seconds; the last 600, recorded while the sim plays or steps); empty if the bus has no such signal.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "t"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let histories = ctx.get::<SignalHistories>()?;
        let samples: Vec<Value> = histories.0.get(&self.name).map_or_default(|samples| {
            samples
                .iter()
                .map(|&(t, v)| json!([f64::from(t), f64::from(v)]))
                .collect()
        });
        Ok(OpOutput::none().returning(Value::Array(samples)))
    }
}

/// Write recorded signals to a CSV file.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SignalExport {
    /// The file (must end in `.csv`; relative to the working directory).
    pub path: String,
    /// The signals, in column order; the first one's samples set the rows.
    pub names: Vec<String>,
}

impl Operation for SignalExport {
    const META: OpMeta = editor(
        "signal.export",
        "signal-export",
        "Write signals' recorded history to a .csv file: a header t,name1,name2,... then one row per sample of the first signal (a cell is empty where another signal has no sample at that t). Returns the path. No undo step.",
        r#"{"path": "target/signals.csv", "names": ["t"]}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let path = PathBuf::from(&self.path);
        let is_csv = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        if !is_csv {
            return Err(OpError::Invalid(format!(
                "an export file ends in .csv: `{}`",
                self.path
            )));
        }
        let histories = ctx.get::<SignalHistories>()?;
        let csv = signals_csv(histories, &self.names)?;
        Ok(OpOutput::none()
            .message(EditorRequest::Export { path, csv })
            .returning(self.path))
    }
}

/// The CSV text of `names`' histories: a header `t,<names>`, then one row per
/// sample of the first signal. Another signal's cell holds its sample at the
/// same simulated time (every signal recorded in one frame shares that
/// frame's stamp), else is empty. A name the bus does not hold is an error:
/// an export of a typo would be a file of empty cells.
pub fn signals_csv(histories: &SignalHistories, names: &[String]) -> Result<String, OpError> {
    let Some(first) = names.first() else {
        return Err(OpError::Invalid("name at least one signal".to_owned()));
    };
    let mut columns = Vec::with_capacity(names.len());
    for name in names {
        if name.contains([',', '"', '\n', '\r']) {
            return Err(OpError::Invalid(format!(
                "a column name has no comma, quote or newline: `{name}`"
            )));
        }
        let Some(samples) = histories.0.get(name) else {
            let mut known: Vec<&str> = histories.0.keys().map(String::as_str).collect();
            known.sort_unstable();
            return Err(OpError::Invalid(format!(
                "no signal `{name}` on the bus (signals: {})",
                known.join(", ")
            )));
        };
        columns.push(samples.as_slice());
    }
    let mut csv = String::from("t");
    for name in names {
        csv.push(',');
        csv.push_str(name);
    }
    csv.push('\n');
    let rows = histories.0.get(first).map_or(&[][..], Vec::as_slice);
    // One cursor per column: times only grow, so each column is walked once.
    let mut cursors = vec![0_usize; columns.len()];
    for &(t, _) in rows {
        csv.push_str(&t.to_string());
        for (column, cursor) in columns.iter().zip(cursors.iter_mut()) {
            csv.push(',');
            while column.get(*cursor).is_some_and(|&(ct, _)| ct < t) {
                *cursor += 1;
            }
            if let Some(&(ct, v)) = column.get(*cursor)
                && ct.to_bits() == t.to_bits()
            {
                csv.push_str(&v.to_string());
                *cursor += 1;
            }
        }
        csv.push('\n');
    }
    Ok(csv)
}

/// Moves each pending [`EditorRequest`] into its table. Every table upserts
/// by name, so re-running a definitions script (a hot reload) converges.
pub fn apply_editor_requests(world: &mut World) {
    for request in gradiance_core::messages::drain::<EditorRequest>(world) {
        match request {
            EditorRequest::Action(action) => {
                if let Some(mut table) = world.get_resource_mut::<ScriptActions>() {
                    table.upsert(action);
                }
            }
            EditorRequest::Tool(tool) => {
                if let Some(mut table) = world.get_resource_mut::<ScriptTools>() {
                    let index = table.upsert(tool);
                    if u16::try_from(index).is_err() {
                        warn!("script tool {index} is past the palette's index range");
                    }
                }
            }
            EditorRequest::Label { name, id } => {
                if let Some(mut table) = world.get_resource_mut::<WorkspaceLabels>() {
                    table.upsert(name, id);
                }
            }
            EditorRequest::Publish { name, value } => publish(world, name, value),
            EditorRequest::Graph(edit) => edit_graph(world, &edit),
            EditorRequest::Export { path, csv } => match std::fs::write(&path, csv) {
                Ok(()) => info!(path = %path.display(), "signals exported"),
                Err(e) => warn!(path = %path.display(), error = %e, "signal export failed"),
            },
        }
    }
}

/// Applies one graph edit: validated as a whole, kept only when it changes
/// the graph (so a no-op re-declaration records no undo step). A refused
/// edit is logged and leaves the graph as it was.
fn edit_graph(world: &mut World, edit: &GraphEdit) {
    let Some(mut graph) = world.get_resource_mut::<gradiance_signal::Graph>() else {
        return;
    };
    let mut next = graph.clone();
    match next.edit(edit) {
        Ok(_) if next != *graph => *graph = next,
        Ok(_) => {}
        Err(error) => warn!(%error, "graph edit refused"),
    }
}

/// Publishes one value on the bus, remembering the name so bus hygiene keeps
/// a script signal alive between runs.
fn publish(world: &mut World, name: String, value: f64) {
    if let Some(mut names) = world.get_resource_mut::<gradiance_signal::ScriptSignals>()
        && !names.0.contains(&name)
    {
        names.0.push(name.clone());
    }
    if let Some(mut bus) = world.get_resource_mut::<gradiance_signal::SignalBus>() {
        bus.publish(&name, value as f32, false);
    }
}
