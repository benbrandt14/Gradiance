//! The **node-graph canvas**: the Simulink-style visual editor for the signal
//! dataflow (`docs/signal-dataflow.md`), built on the [`egui-snarl`] node-graph
//! widget (the one major node editor tracking our pinned egui 0.35 —
//! `egui_node_graph2` is stuck on 0.29). snarl owns the box layout, pan/zoom,
//! and the drag-to-connect interaction; this module is the **adapter** between
//! it and the ECS dataflow.
//!
//! **Objects are the nodes** (Algodoo per-object behavior + Simulink blocks): a
//! body appears as a block whose **outputs are its sensor ports** (speed,
//! height, contact force, …) and **inputs are its actuator ports** (fill,
//! tracer color). Params and computed signals are their own producer/modulator
//! blocks. A **wire is a [`SignalBinding`]**: dragging a producer output onto a
//! body's actuator input creates one (source → sink); removing the wire deletes
//! it. Bindings are the single config-seam currency (edited directly, persisted)
//! — there is no separate placeable sensor/actuator entity anymore.
//!
//! The graph is *derived from the scene* every frame by `reconcile`: a node per
//! body **explicitly added** ("Add to node editor") or referenced by a binding
//! — never merely selected, so the canvas stays uncluttered — plus every param
//! and computed signal, keyed by `GraphKey` so dragged positions persist; the
//! wires are rebuilt from the bindings. It docks to the screen bottom.
//!
//! [`egui-snarl`]: https://crates.io/crates/egui-snarl
//! [`SignalBinding`]: crate::domain::signal::SignalBinding

use crate::core::ids::{IdIndex, StableId};
use crate::domain::Body;
use crate::interaction::selection::Selection;
use crate::physics::queries::PhysicsQueries;
use crate::signal::{
    BlockOp, ComputedSignals, GradientSpec, SignalBinding, SignalBindings, SignalBus, SignalMap,
    SignalParam, SignalParams, SignalSink, SignalSource, read_source,
};
use crate::ui::ports::{body_actuators, body_sensors};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer, WireStyle};
use egui_snarl::{InPin, InPinId, NodeId, OutPin, OutPinId, Snarl};
use std::collections::{HashMap, HashSet};

/// Identity of a block on the canvas, so reconciliation matches a snarl node
/// to its dataflow source across frames.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum GraphKey {
    /// A scene body (its sensor/actuator ports), by id.
    Body(StableId),
    /// A `defparam` knob, by name.
    Param(String),
    /// A `defsignal` modulator, by name.
    Computed(String),
    /// The singleton **Scope** sink — the Live Plot as a block. Any signal
    /// wired into it becomes a plot-sink binding.
    Scope,
}

/// What a block *is* — carries what the viewer needs to label pins and resolve
/// a pin back to a [`SignalSource`] / [`SignalSink`].
#[derive(Clone)]
enum NodeData {
    /// A body block: outputs are `body_sensors`, inputs are `body_actuators`.
    /// `name` is an optional custom short label (else the block shows "body").
    Body { id: StableId, name: Option<String> },
    /// A param block: one output (its published bus name).
    Param(String),
    /// A computed/modulation block. `block` is the structured op (canvas
    /// blocks) whose operand pins are wireable; `inputs` is the read-only pin
    /// list for a console-authored RPN signal (`block` = `None`).
    Computed {
        name: String,
        inputs: Vec<String>,
        block: Option<BlockOp>,
    },
    /// The singleton **Scope** block: one input, no output — wiring a signal
    /// into it makes a `SignalSink::Plot` binding (the Live Plot as a block).
    Scope,
}

impl NodeData {
    fn key(&self) -> GraphKey {
        match self {
            Self::Body { id, .. } => GraphKey::Body(*id),
            Self::Param(name) => GraphKey::Param(name.clone()),
            Self::Computed { name, .. } => GraphKey::Computed(name.clone()),
            Self::Scope => GraphKey::Scope,
        }
    }

    fn role(&self) -> Role {
        match self {
            Self::Body { .. } => Role::Body,
            Self::Param(_) => Role::Producer,
            Self::Computed { .. } => Role::Modulator,
            Self::Scope => Role::Scope,
        }
    }

    /// The short title — the block's *type*, or its custom name if set.
    fn title(&self) -> String {
        match self {
            Self::Body { name, .. } => name.clone().unwrap_or_else(|| "body".to_owned()),
            Self::Param(name) => format!("⊙ {name}"),
            Self::Computed { name, .. } => format!("ƒ {name}"),
            Self::Scope => "▭ Scope".to_owned(),
        }
    }

    /// The [`SignalSource`] a given output pin publishes (for wiring).
    fn output_source(&self, index: usize) -> Option<SignalSource> {
        match self {
            Self::Body { id, .. } => body_sensors(*id).get(index).map(|(_, s)| s.clone()),
            Self::Param(name) => (index == 0).then(|| SignalSource::Named(name.clone())),
            Self::Computed { name, .. } => (index == 0).then(|| SignalSource::Named(name.clone())),
            Self::Scope => None,
        }
    }

    /// The [`SignalSink`] a given input pin drives.
    fn input_sink(&self, index: usize) -> Option<SignalSink> {
        match self {
            Self::Body { id, .. } => body_actuators(*id).get(index).map(|(_, s)| s.clone()),
            Self::Scope => (index == 0).then_some(SignalSink::Plot),
            _ => None,
        }
    }
}

/// Column/color role of a block.
#[derive(Clone, Copy)]
enum Role {
    Producer,
    Modulator,
    Body,
    Scope,
}

impl Role {
    fn column_x(self) -> f32 {
        match self {
            Self::Producer => 20.0,
            Self::Modulator => 220.0,
            Self::Body => 420.0,
            Self::Scope => 640.0,
        }
    }
}

/// The node-graph canvas state: visibility, the snarl graph, and the
/// `GraphKey` → `NodeId` map reconciliation keeps in sync. Pure editor
/// view-state — never authored, never persisted.
#[derive(Resource)]
pub struct NodeGraph {
    open: bool,
    snarl: Snarl<NodeData>,
    keys: HashMap<GraphKey, NodeId>,
    /// Bodies explicitly added to the canvas ("Add to node editor"). Bodies
    /// don't auto-appear on selection — only when added or when a binding
    /// references them — so the canvas stays uncluttered.
    added: HashSet<StableId>,
    /// Optional custom short names per body (editable in the block; view-state).
    names: HashMap<StableId, String>,
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            open: false,
            snarl: Snarl::new(),
            keys: HashMap::new(),
            added: HashSet::new(),
            names: HashMap::new(),
        }
    }
}

impl NodeGraph {
    /// Whether the canvas is shown (read by the toolbar toggle).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the canvas visibility.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Adds a body's block to the canvas and opens it (the "Add to node
    /// editor" context action).
    pub fn add_body(&mut self, id: StableId) {
        self.added.insert(id);
        self.open = true;
    }
}

/// The index of the sensor output pin a scene [`SignalSource`] maps to — must
/// match the order of [`body_sensors`]. `None` for sources that aren't a
/// single body's sensor port (distance, named).
fn sensor_index(source: &SignalSource) -> Option<usize> {
    match source {
        SignalSource::Speed(_) => Some(0),
        SignalSource::Spin(_) => Some(1),
        SignalSource::Height(_) => Some(2),
        SignalSource::PosX(_) => Some(3),
        SignalSource::ContactForce(_) => Some(4),
        SignalSource::ContactCount(_) => Some(5),
        SignalSource::Distance(..) | SignalSource::Named(_) => None,
    }
}

/// The `(GraphKey, output index)` a binding source resolves to, if it maps to
/// a producer block on the canvas.
fn source_pin(source: &SignalSource) -> Option<(GraphKey, usize)> {
    match source {
        SignalSource::Named(name) => Some((GraphKey::Param(name.clone()), 0)),
        SignalSource::Distance(..) => None,
        other => {
            let id = *match other {
                SignalSource::Speed(id)
                | SignalSource::Spin(id)
                | SignalSource::Height(id)
                | SignalSource::PosX(id)
                | SignalSource::ContactForce(id)
                | SignalSource::ContactCount(id) => id,
                _ => return None,
            };
            Some((GraphKey::Body(id), sensor_index(other)?))
        }
    }
}

/// The `(GraphKey, input index)` a binding sink resolves to — body actuator
/// pins, or the Scope block's input for `Plot`.
fn sink_pin(sink: &SignalSink) -> (GraphKey, usize) {
    match sink {
        SignalSink::Fill(id) => (GraphKey::Body(*id), 0),
        SignalSink::TracerColor(id) => (GraphKey::Body(*id), 1),
        SignalSink::Plot => (GraphKey::Scope, 0),
    }
}

/// Renders the node-graph canvas. Reconciles blocks + wires from the live
/// dataflow, shows it, and applies any wire the user made/removed to the
/// bindings (config-seam direct edit, like the Signals dock).
pub fn node_graph_panel(
    mut contexts: EguiContexts,
    mut graph: ResMut<NodeGraph>,
    mut bindings: ResMut<SignalBindings>,
    mut params: ResMut<SignalParams>,
    mut computed: ResMut<ComputedSignals>,
    bodies: Query<&StableId, With<Body>>,
    mut selection: ResMut<Selection>,
    ids: Query<&StableId>,
    bus: Res<SignalBus>,
    physics: PhysicsQueries,
    body_transforms: Query<&Transform, With<Body>>,
    index: Res<IdIndex>,
    fixed: Res<Time<Fixed>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !graph.open {
        return Ok(());
    }

    let desired = collect_desired(
        &graph.added,
        &graph.names,
        &bindings,
        &params,
        &computed,
        &bodies,
    );
    reconcile(&mut graph, desired);
    rebuild_wires(&mut graph, &bindings);

    // Precompute the live value of every output pin (the Simulink "watch the
    // signal flow" readout): sensors through the shared reader, params/computed
    // off the bus.
    let dt = fixed.timestep().as_secs_f32().max(1e-6);
    let mut output_values: HashMap<GraphKey, Vec<Option<f32>>> = HashMap::new();
    for (_, data) in graph.snarl.node_ids() {
        let vals: Vec<Option<f32>> = match data {
            NodeData::Body { id, .. } => body_sensors(*id)
                .iter()
                .map(|(_, s)| read_source(s, &index, &physics, &body_transforms, dt))
                .collect(),
            NodeData::Param(name) | NodeData::Computed { name, .. } => vec![bus.get(name)],
            NodeData::Scope => Vec::new(),
        };
        output_values.insert(data.key(), vals);
    }
    // The value driving each bound actuator/scope input (from its binding).
    let input_values = binding_input_values(&bindings, &bus);

    // Scene selection → highlight the matching body blocks.
    let selected: HashSet<StableId> = selection
        .iter()
        .filter_map(|e| ids.get(e).ok().copied())
        .collect();
    let mut viewer = GraphViewer {
        selected,
        output_values,
        input_values,
        bindings: bindings.0.clone(),
        params: params.0.clone(),
        ..GraphViewer::default()
    };
    let style = SnarlStyle {
        // Simulink-style right-angle wires — they route around a block instead
        // of behind it (a body's own sensor→actuator self-wire).
        wire_style: Some(WireStyle::AxisAligned { corner_radius: 8.0 }),
        ..SnarlStyle::new()
    };
    // A screen-bottom dock (same background-layer root pattern the right dock
    // uses, so panels claim edges rather than float).
    let mut root = egui::Ui::new(
        ctx.clone(),
        "node-graph-root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::bottom("node-graph-dock")
        .resizable(true)
        .default_size(260.0)
        .show(&mut root, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Node Graph").strong());
                ui.weak(
                    "· right-click a body ▸ Add to node editor · drag sensor → actuator to bind",
                );
            });
            if graph.keys.is_empty() {
                ui.weak("Empty — add a body from its right-click menu, or add params/computed.");
                return;
            }
            graph.snarl.show(&mut viewer, &style, "node-graph", ui);
        });

    // A "locate" click selects the body in the scene (the inspector follows).
    if let Some(id) = viewer.select_body
        && let Some(entity) = index.entity(id)
    {
        selection.set(entity);
    }
    apply_viewer_edits(
        viewer,
        &mut graph,
        &mut bindings,
        &mut params,
        &mut computed,
    );
    Ok(())
}

/// Drains a frame's [`GraphViewer`] edits into the config-seam resources:
/// binding create/remove/transfer, custom names, and in-canvas authoring
/// (add param, remove body, delete param/computed).
fn apply_viewer_edits(
    viewer: GraphViewer,
    graph: &mut NodeGraph,
    bindings: &mut SignalBindings,
    params: &mut SignalParams,
    computed: &mut ComputedSignals,
) {
    for binding in viewer.new_bindings {
        bindings.0.push(binding);
    }
    for (source, sink) in viewer.removed {
        bindings
            .0
            .retain(|b| !(b.source == source && b.sink == sink));
    }
    for (source, sink, map, gradient) in viewer.transfer_edits {
        if let Some(binding) = bindings
            .0
            .iter_mut()
            .find(|b| b.source == source && b.sink == sink)
        {
            binding.map = map;
            binding.gradient = gradient;
        }
    }
    for (id, name) in viewer.renames {
        if name.trim().is_empty() {
            graph.names.remove(&id);
        } else {
            graph.names.insert(id, name);
        }
    }
    for (name, value) in viewer.param_edits {
        if let Some(param) = params.0.iter_mut().find(|p| p.name == name) {
            param.value = value;
        }
    }
    if viewer.add_param {
        let name = fresh_param_name(params);
        params.0.push(crate::signal::SignalParam::unit(name));
    }
    for op in viewer.add_blocks {
        let name = fresh_block_name(&op, computed);
        computed
            .0
            .push(crate::signal::ComputedSignal::from_block(name, op));
    }
    // Operand wiring + constant edits: replace the block's op and re-lower.
    for (name, op) in viewer.block_edits {
        if let Some(signal) = computed.0.iter_mut().find(|c| c.name == name) {
            signal.block = Some(op);
            signal.relower();
        }
    }
    for id in viewer.remove_bodies {
        graph.added.remove(&id);
    }
    for name in viewer.delete_params {
        params.0.retain(|p| p.name != name);
    }
    for name in viewer.delete_computed {
        computed.0.retain(|c| c.name != name);
    }
}

/// A fresh unique `param-N` name over the existing params.
fn fresh_param_name(params: &SignalParams) -> String {
    (1..=u32::try_from(params.0.len()).unwrap_or(0) + 1)
        .map(|n| format!("param-{n}"))
        .find(|name| !params.0.iter().any(|p| &p.name == name))
        .unwrap_or_else(|| "param".to_owned())
}

/// Builds the desired block set: a node per **explicitly added** body or body
/// **referenced by a binding** (never merely selected — that would clutter the
/// canvas), plus every param and computed signal.
fn collect_desired(
    added: &HashSet<StableId>,
    names: &HashMap<StableId, String>,
    bindings: &SignalBindings,
    params: &SignalParams,
    computed: &ComputedSignals,
    bodies: &Query<&StableId, With<Body>>,
) -> Vec<NodeData> {
    let live: HashSet<StableId> = bodies.iter().copied().collect();
    let mut shown: HashSet<StableId> = added
        .iter()
        .copied()
        .filter(|id| live.contains(id))
        .collect();
    // Bodies referenced by a binding (a wire needs its endpoints on-canvas).
    for binding in &bindings.0 {
        for id in binding_bodies(binding) {
            if live.contains(&id) {
                shown.insert(id);
            }
        }
    }
    // Bodies feeding a modulation block (a canonical sensor name in its expr).
    for signal in &computed.0 {
        for input in signal.expr.inputs() {
            if let Some(source) = SignalSource::from_bus_name(&input)
                && let Some((GraphKey::Body(id), _)) = source_pin(&source)
                && live.contains(&id)
            {
                shown.insert(id);
            }
        }
    }

    let mut items: Vec<NodeData> = Vec::new();
    for param in &params.0 {
        items.push(NodeData::Param(param.name.clone()));
    }
    for signal in &computed.0 {
        items.push(NodeData::Computed {
            name: signal.name.clone(),
            inputs: signal.expr.inputs(),
            block: signal.block.clone(),
        });
    }
    for id in shown {
        items.push(NodeData::Body {
            id,
            name: names.get(&id).cloned(),
        });
    }
    // The Scope sink is a fixture whenever there's anything to plot into it.
    if !items.is_empty() {
        items.push(NodeData::Scope);
    }
    items
}

/// The live value driving each bound input pin — `map[key][input]` is the bus
/// value of the binding whose sink is that pin (Simulink's value-on-the-wire).
fn binding_input_values(
    bindings: &SignalBindings,
    bus: &SignalBus,
) -> HashMap<GraphKey, Vec<Option<f32>>> {
    let mut map: HashMap<GraphKey, Vec<Option<f32>>> = HashMap::new();
    for binding in &bindings.0 {
        let (key, input) = sink_pin(&binding.sink);
        let slot = map.entry(key).or_default();
        if slot.len() <= input {
            slot.resize(input + 1, None);
        }
        slot[input] = bus.get(&binding.name);
    }
    map
}

/// The bodies a binding references (source + sink).
fn binding_bodies(binding: &SignalBinding) -> Vec<StableId> {
    let mut ids = Vec::new();
    match &binding.source {
        SignalSource::Speed(id)
        | SignalSource::Spin(id)
        | SignalSource::Height(id)
        | SignalSource::PosX(id)
        | SignalSource::ContactForce(id)
        | SignalSource::ContactCount(id) => ids.push(*id),
        SignalSource::Distance(a, b) => {
            ids.push(*a);
            ids.push(*b);
        }
        SignalSource::Named(_) => {}
    }
    match &binding.sink {
        SignalSink::Fill(id) | SignalSink::TracerColor(id) => ids.push(*id),
        SignalSink::Plot => {}
    }
    ids
}

/// The per-column running y-cursor, so freshly appearing blocks stack down
/// their column instead of overlapping.
#[derive(Default)]
struct ColumnCursor {
    producer: f32,
    modulator: f32,
    body: f32,
    scope: f32,
}

impl ColumnCursor {
    fn next(&mut self, role: Role) -> egui::Pos2 {
        let slot = match role {
            Role::Producer => &mut self.producer,
            Role::Modulator => &mut self.modulator,
            Role::Body => &mut self.body,
            Role::Scope => &mut self.scope,
        };
        let y = *slot + 16.0;
        *slot = y + 96.0;
        egui::pos2(role.column_x(), y)
    }
}

/// Rebuilds the snarl graph to mirror `desired`, preserving dragged positions,
/// then re-draws the wires from the bindings + computed-input name matches.
fn reconcile(graph: &mut NodeGraph, desired: Vec<NodeData>) {
    let mut cursor = ColumnCursor::default();
    let mut seen: HashSet<GraphKey> = HashSet::with_capacity(desired.len());
    for node in desired {
        let key = node.key();
        seen.insert(key.clone());
        if let Some(&id) = graph.keys.get(&key)
            && let Some(slot) = graph.snarl.get_node_mut(id)
        {
            *slot = node;
            continue;
        }
        let pos = cursor.next(node.role());
        let id = graph.snarl.insert_node(pos, node);
        graph.keys.insert(key, id);
    }
    let stale: Vec<(GraphKey, NodeId)> = graph
        .keys
        .iter()
        .filter(|(key, _)| !seen.contains(*key))
        .map(|(key, id)| (key.clone(), *id))
        .collect();
    for (key, id) in stale {
        if graph.snarl.get_node(id).is_some() {
            graph.snarl.remove_node(id);
        }
        graph.keys.remove(&key);
    }
}

/// The snarl viewer: labels each block's pins and turns user connect/disconnect
/// gestures into binding edits (collected, applied by the system). Wires shown
/// are the current bindings; it never owns the graph.
#[derive(Default)]
struct GraphViewer {
    /// Scene-selected body ids — their blocks get a highlight frame.
    selected: HashSet<StableId>,
    /// Live value of each output pin, `[key][output]` (the running readout).
    output_values: HashMap<GraphKey, Vec<Option<f32>>>,
    /// Live value driving each bound input pin, `[key][input]`.
    input_values: HashMap<GraphKey, Vec<Option<f32>>>,
    /// A snapshot of the bindings, so a body footer can render + edit the
    /// transfer (domain + gradient) of the wires driving it.
    bindings: Vec<SignalBinding>,
    /// A snapshot of the params, so a param block's footer can tune its value
    /// in place (the same knob the Signals dock exposes).
    params: Vec<SignalParam>,
    /// Param value edits `(name, new value)` from a param block's footer.
    param_edits: Vec<(String, f32)>,
    new_bindings: Vec<SignalBinding>,
    removed: Vec<(SignalSource, SignalSink)>,
    /// Transfer edits `(source, sink, map, gradient)` from a body footer.
    transfer_edits: Vec<(SignalSource, SignalSink, SignalMap, GradientSpec)>,
    /// Custom-name edits `(body, new name)` from the block's rename field.
    renames: Vec<(StableId, String)>,
    /// A body block whose "locate" button was clicked — select it in the
    /// scene (block → object → inspector, the reverse of the highlight).
    select_body: Option<StableId>,
    /// In-canvas authoring requests (applied by the system).
    add_param: bool,
    /// Add a computed/modulation block from a template op.
    add_blocks: Vec<BlockOp>,
    /// Replace a computed block's op (operand wiring + footer constants).
    block_edits: Vec<(String, BlockOp)>,
    remove_bodies: Vec<StableId>,
    delete_params: Vec<String>,
    delete_computed: Vec<String>,
}

impl GraphViewer {
    /// Resolves an output pin to the [`SignalSource`] it publishes.
    fn out_source(snarl: &Snarl<NodeData>, pin: OutPinId) -> Option<SignalSource> {
        snarl.get_node(pin.node)?.output_source(pin.output)
    }

    /// Resolves an input pin to the [`SignalSink`] it drives.
    fn in_sink(snarl: &Snarl<NodeData>, pin: InPinId) -> Option<SignalSink> {
        snarl.get_node(pin.node)?.input_sink(pin.input)
    }

    /// The bus name an output pin publishes under, so it can feed a modulation
    /// operand: a named producer's name, or a body sensor's canonical name.
    fn out_source_name(snarl: &Snarl<NodeData>, pin: OutPinId) -> Option<String> {
        match Self::out_source(snarl, pin)? {
            SignalSource::Named(name) => Some(name),
            other => other.bus_name(),
        }
    }

    /// If `pin` is a modulation block's operand input, returns `(name, op)` so
    /// the caller can rewrite its operand and re-lower.
    fn block_at(snarl: &Snarl<NodeData>, pin: InPinId) -> Option<(String, BlockOp)> {
        match snarl.get_node(pin.node) {
            Some(NodeData::Computed {
                name,
                block: Some(op),
                ..
            }) => Some((name.clone(), op.clone())),
            _ => None,
        }
    }

    /// The precomputed live value at `(key, pin)` from `values`.
    fn value_at(
        values: &HashMap<GraphKey, Vec<Option<f32>>>,
        key: &GraphKey,
        pin: usize,
    ) -> Option<f32> {
        values.get(key).and_then(|v| v.get(pin).copied()).flatten()
    }
}

/// Appends a live value readout to a pin label (Simulink's on-wire value).
fn pin_label(name: &str, value: Option<f32>) -> String {
    match value {
        Some(v) => format!("{name}  {v:.2}"),
        None => name.to_owned(),
    }
}

impl SnarlViewer<NodeData> for GraphViewer {
    fn title(&mut self, node: &NodeData) -> String {
        node.title()
    }

    fn node_frame(
        &mut self,
        default: egui::Frame,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<NodeData>,
    ) -> egui::Frame {
        if let Some(NodeData::Body { id, .. }) = snarl.get_node(node)
            && self.selected.contains(id)
        {
            return default.stroke(egui::Stroke::new(
                2.0,
                egui::Color32::from_rgb(120, 200, 255),
            ));
        }
        default
    }

    fn has_body(&mut self, node: &NodeData) -> bool {
        matches!(node, NodeData::Body { .. })
    }

    fn show_body(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) {
        let Some(NodeData::Body { id, name }) = snarl.get_node(node) else {
            return;
        };
        let (id, mut text) = (*id, name.clone().unwrap_or_default());
        ui.horizontal(|ui| {
            // Locate: select this body in the scene (block → object →
            // inspector), the reverse of the scene-selection highlight.
            if ui
                .small_button("⌖")
                .on_hover_text("Select this object in the scene")
                .clicked()
            {
                self.select_body = Some(id);
            }
            if ui
                .add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(72.0)
                        .hint_text("name"),
                )
                .changed()
            {
                self.renames.push((id, text));
            }
        });
    }

    fn inputs(&mut self, node: &NodeData) -> usize {
        match node {
            NodeData::Body { id, .. } => body_actuators(*id).len(),
            NodeData::Param(_) => 0,
            NodeData::Computed { inputs, block, .. } => block
                .as_ref()
                .map_or(inputs.len(), |op| op.input_slots().len()),
            NodeData::Scope => 1,
        }
    }

    fn outputs(&mut self, node: &NodeData) -> usize {
        match node {
            NodeData::Body { id, .. } => body_sensors(*id).len(),
            NodeData::Param(_) | NodeData::Computed { .. } => 1,
            NodeData::Scope => 0,
        }
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let name = match snarl.get_node(pin.id.node) {
            Some(NodeData::Body { id, .. }) => body_actuators(*id)
                .get(pin.id.input)
                .map_or_else(String::new, |(l, _)| (*l).to_owned()),
            Some(NodeData::Computed {
                inputs,
                block: None,
                ..
            }) => inputs.get(pin.id.input).cloned().unwrap_or_default(),
            Some(NodeData::Computed {
                block: Some(op), ..
            }) => op
                .input_slots()
                .get(pin.id.input)
                .map_or_else(String::new, |(l, _)| (*l).to_owned()),
            Some(NodeData::Scope) => "signals".to_owned(),
            _ => String::new(),
        };
        let key = snarl.get_node(pin.id.node).map(NodeData::key);
        let value = key
            .as_ref()
            .and_then(|k| Self::value_at(&self.input_values, k, pin.id.input));
        ui.label(pin_label(&name, value));
        PinInfo::circle().with_fill(egui::Color32::from_rgb(210, 170, 120))
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let name = match snarl.get_node(pin.id.node) {
            Some(NodeData::Body { id, .. }) => body_sensors(*id)
                .get(pin.id.output)
                .map_or_else(String::new, |(l, _)| (*l).to_owned()),
            Some(NodeData::Param(name) | NodeData::Computed { name, .. }) => name.clone(),
            Some(NodeData::Scope) | None => String::new(),
        };
        let key = snarl.get_node(pin.id.node).map(NodeData::key);
        let value = key
            .as_ref()
            .and_then(|k| Self::value_at(&self.output_values, k, pin.id.output));
        ui.label(pin_label(&name, value));
        PinInfo::circle().with_fill(egui::Color32::from_rgb(140, 220, 150))
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeData>) {
        // A wire into a modulation block operand: rewrite that operand to the
        // producer's bus name (a named producer only) and re-lower.
        if let Some((name, mut op)) = Self::block_at(snarl, to.id) {
            if let Some(src) = Self::out_source_name(snarl, from.id) {
                op.set_input(to.id.input, Some(src));
                self.block_edits.push((name, op));
            }
            return;
        }
        // Otherwise a producer output → a body actuator / scope input: create a
        // binding. Reconciliation redraws the wire once it lands.
        if let (Some(source), Some(sink)) = (
            Self::out_source(snarl, from.id),
            Self::in_sink(snarl, to.id),
        ) {
            // Don't create a second identical wire.
            let exists = self
                .bindings
                .iter()
                .chain(self.new_bindings.iter())
                .any(|b| b.source == source && b.sink == sink);
            if !exists {
                self.new_bindings.push(SignalBinding {
                    name: format!("wire-{}", self.new_bindings.len() + 1),
                    source,
                    map: SignalMap::default(),
                    gradient: GradientSpec::default(),
                    sink,
                });
            }
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeData>) {
        if let Some((name, mut op)) = Self::block_at(snarl, to.id) {
            op.set_input(to.id.input, None);
            self.block_edits.push((name, op));
            return;
        }
        if let (Some(source), Some(sink)) = (
            Self::out_source(snarl, from.id),
            Self::in_sink(snarl, to.id),
        ) {
            self.removed.push((source, sink));
        }
    }

    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<NodeData>) {
        if let Some(sink) = Self::in_sink(snarl, pin.id) {
            for remote in &pin.remotes {
                if let Some(source) = Self::out_source(snarl, *remote) {
                    self.removed.push((source, sink.clone()));
                }
            }
        }
    }

    // In-canvas authoring: right-click the background to add a producer.
    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<NodeData>) -> bool {
        true
    }

    fn show_graph_menu(
        &mut self,
        _pos: egui::Pos2,
        ui: &mut egui::Ui,
        _snarl: &mut Snarl<NodeData>,
    ) {
        ui.label(egui::RichText::new("Add block").strong());
        ui.menu_button("Input", |ui| {
            if ui.button("⊙ Parameter (slider)").clicked() {
                self.add_param = true;
                ui.close();
            }
            if ui.button("# Constant").clicked() {
                self.add_blocks.push(BlockOp::Constant { value: 1.0 });
                ui.close();
            }
            if ui.button("ƒ Time").clicked() {
                self.add_blocks.push(BlockOp::Time);
                ui.close();
            }
            if ui.button("ƒ Oscillator").clicked() {
                self.add_blocks.push(BlockOp::Oscillator {
                    amp: 1.0,
                    freq: 1.0,
                });
                ui.close();
            }
        });
        ui.menu_button("Modulation", |ui| {
            let mut add = |ui: &mut egui::Ui, label: &str, op: BlockOp| {
                if ui.button(label).clicked() {
                    self.add_blocks.push(op);
                    ui.close();
                }
            };
            add(
                ui,
                "ƒ Gain (× k)",
                BlockOp::Gain {
                    input: None,
                    k: 1.0,
                },
            );
            add(ui, "ƒ Sum (a + b)", BlockOp::Sum { a: None, b: None });
            add(ui, "ƒ Sub (a − b)", BlockOp::Sub { a: None, b: None });
            add(
                ui,
                "ƒ Product (a · b)",
                BlockOp::Product { a: None, b: None },
            );
            add(ui, "ƒ Min (a, b)", BlockOp::Min { a: None, b: None });
            add(ui, "ƒ Max (a, b)", BlockOp::Max { a: None, b: None });
            add(ui, "ƒ Abs |in|", BlockOp::Abs { input: None });
        });
        ui.weak("Output: the ▭ Scope block is always present.");
    }

    // Right-click a block to remove/delete it.
    fn has_node_menu(&mut self, _node: &NodeData) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) {
        match snarl.get_node(node) {
            Some(NodeData::Body { id, .. }) => {
                let id = *id;
                if ui.button("Remove from node editor").clicked() {
                    self.remove_bodies.push(id);
                    ui.close();
                }
            }
            Some(NodeData::Param(name)) => {
                let name = name.clone();
                if ui.button("Delete parameter").clicked() {
                    self.delete_params.push(name);
                    ui.close();
                }
            }
            Some(NodeData::Computed { name, .. }) => {
                let name = name.clone();
                if ui.button("Delete computed").clicked() {
                    self.delete_computed.push(name);
                    ui.close();
                }
            }
            Some(NodeData::Scope) | None => {
                ui.weak("the plot output");
            }
        }
    }

    // A footer configures a block in place: a body edits the transfer of each
    // wire driving it; a modulation block edits its constants (k / amp / freq).
    fn has_footer(&mut self, node: &NodeData) -> bool {
        matches!(
            node,
            NodeData::Body { .. } | NodeData::Param(_) | NodeData::Computed { block: Some(_), .. }
        )
    }

    fn show_footer(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) {
        // A modulation block's constants.
        if let Some(NodeData::Computed {
            name,
            block: Some(op),
            ..
        }) = snarl.get_node(node)
        {
            let name = name.clone();
            let mut op = op.clone();
            if block_constants(ui, &mut op) {
                self.block_edits.push((name, op));
            }
            return;
        }
        // A param block's value — the same tunable knob as the Signals dock,
        // edited in place (Simulink source-block-style).
        if let Some(NodeData::Param(name)) = snarl.get_node(node) {
            if let Some(param) = self.params.iter().find(|p| &p.name == name) {
                let mut value = param.value;
                if ui
                    .add(egui::Slider::new(&mut value, param.min..=param.max))
                    .changed()
                {
                    self.param_edits.push((param.name.clone(), value));
                }
            }
            return;
        }
        let Some(NodeData::Body { id, .. }) = snarl.get_node(node) else {
            return;
        };
        let id = *id;
        for binding in &self.bindings {
            let (key, _) = sink_pin(&binding.sink);
            if key != GraphKey::Body(id) {
                continue;
            }
            let mut map = binding.map;
            let mut gradient = binding.gradient;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.small(sink_label(&binding.sink));
                changed |= ui
                    .add(egui::DragValue::new(&mut map.in_min).speed(1.0).prefix("["))
                    .changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut map.in_max).speed(1.0).suffix("]"))
                    .changed();
                egui::ComboBox::from_id_salt(ui.id().with(("grad", &binding.name)))
                    .selected_text(format!("{gradient:?}"))
                    .show_ui(ui, |ui| {
                        for option in GradientSpec::ALL {
                            changed |= ui
                                .selectable_value(&mut gradient, option, format!("{option:?}"))
                                .changed();
                        }
                    });
            });
            if changed {
                self.transfer_edits.push((
                    binding.source.clone(),
                    binding.sink.clone(),
                    map,
                    gradient,
                ));
            }
        }
    }
}

/// A short label for an actuator sink channel.
fn sink_label(sink: &SignalSink) -> &'static str {
    match sink {
        SignalSink::Fill(_) => "fill",
        SignalSink::TracerColor(_) => "tracer",
        SignalSink::Plot => "plot",
    }
}

/// Edits a modulation block's scalar constants in its footer; returns whether
/// anything changed.
fn block_constants(ui: &mut egui::Ui, op: &mut BlockOp) -> bool {
    let mut changed = false;
    match op {
        BlockOp::Gain { k, .. } => {
            ui.horizontal(|ui| {
                ui.small("k");
                changed |= ui.add(egui::DragValue::new(k).speed(0.05)).changed();
            });
        }
        BlockOp::Oscillator { amp, freq } => {
            ui.horizontal(|ui| {
                ui.small("amp");
                changed |= ui.add(egui::DragValue::new(amp).speed(0.05)).changed();
                ui.small("freq");
                changed |= ui.add(egui::DragValue::new(freq).speed(0.05)).changed();
            });
        }
        BlockOp::Constant { value } => {
            ui.horizontal(|ui| {
                ui.small("value");
                changed |= ui.add(egui::DragValue::new(value).speed(0.05)).changed();
            });
        }
        // Ops without editable constants (their behavior is fully wired).
        BlockOp::Time
        | BlockOp::Sum { .. }
        | BlockOp::Product { .. }
        | BlockOp::Sub { .. }
        | BlockOp::Abs { .. }
        | BlockOp::Min { .. }
        | BlockOp::Max { .. } => {}
    }
    changed
}

/// A fresh unique block name over the existing computed signals, seeded from
/// the op's type label (e.g. `gain-1`).
fn fresh_block_name(op: &BlockOp, computed: &ComputedSignals) -> String {
    let base = op.label();
    (1..=u32::try_from(computed.0.len()).unwrap_or(0) + 1)
        .map(|n| format!("{base}-{n}"))
        .find(|name| !computed.0.iter().any(|c| &c.name == name))
        .unwrap_or_else(|| base.to_owned())
}

/// Rebuilds the snarl wires to mirror the bindings + computed-input name
/// matches. Called each frame after reconcile so the visible wires always
/// equal the dataflow. Kept separate for unit testing.
fn rebuild_wires(graph: &mut NodeGraph, bindings: &SignalBindings) {
    let existing: Vec<(OutPinId, InPinId)> = graph.snarl.wires().collect();
    for (out, in_) in existing {
        graph.snarl.disconnect(out, in_);
    }
    // Binding wires: source output pin → sink input pin.
    for binding in &bindings.0 {
        let Some((src_key, out)) = source_pin(&binding.source) else {
            continue;
        };
        let (sink_key, in_) = sink_pin(&binding.sink);
        // A named source may be a param or a computed block.
        let src = graph.keys.get(&src_key).or_else(|| match &binding.source {
            SignalSource::Named(name) => graph.keys.get(&GraphKey::Computed(name.clone())),
            _ => None,
        });
        if let (Some(&src_id), Some(&sink_id)) = (src, graph.keys.get(&sink_key)) {
            graph.snarl.connect(
                OutPinId {
                    node: src_id,
                    output: out,
                },
                InPinId {
                    node: sink_id,
                    input: in_,
                },
            );
        }
    }
    // Modulation-block operand wires: a producer output → the block input each
    // operand names. Read from each block's structured op.
    let operands: Vec<(NodeId, usize, String)> = graph
        .snarl
        .node_ids()
        .filter_map(|(id, data)| match data {
            NodeData::Computed {
                block: Some(op), ..
            } => Some((id, op.clone())),
            _ => None,
        })
        .flat_map(|(id, op)| {
            op.input_slots()
                .into_iter()
                .enumerate()
                .filter_map(move |(slot, (_, name))| name.map(|n| (id, slot, n)))
                .collect::<Vec<_>>()
        })
        .collect();
    for (block_id, slot, name) in operands {
        // A named producer is a param or another computed/modulation block; a
        // canonical sensor name resolves to a body's sensor output pin.
        let src = graph
            .keys
            .get(&GraphKey::Param(name.clone()))
            .or_else(|| graph.keys.get(&GraphKey::Computed(name.clone())))
            .copied()
            .map(|id| (id, 0usize))
            .or_else(|| {
                let source = SignalSource::from_bus_name(&name)?;
                let (key, out) = source_pin(&source)?;
                graph.keys.get(&key).copied().map(|id| (id, out))
            });
        if let Some((src_id, out)) = src {
            graph.snarl.connect(
                OutPinId {
                    node: src_id,
                    output: out,
                },
                InPinId {
                    node: block_id,
                    input: slot,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(graph: &mut NodeGraph, id: StableId) {
        reconcile_add(graph, NodeData::Body { id, name: None });
    }

    fn reconcile_add(graph: &mut NodeGraph, node: NodeData) {
        let key = node.key();
        let pos = egui::pos2(0.0, 0.0);
        let node_id = graph.snarl.insert_node(pos, node);
        graph.keys.insert(key, node_id);
    }

    #[test]
    fn a_binding_draws_a_wire_between_the_right_ports() {
        let a = StableId::new();
        let b = StableId::new();
        let mut graph = NodeGraph::default();
        body(&mut graph, a);
        body(&mut graph, b);
        // A speed→fill binding from A to B.
        let mut bindings = SignalBindings::default();
        bindings.0.push(SignalBinding {
            name: "w".into(),
            source: SignalSource::Speed(a),
            map: SignalMap::default(),
            gradient: GradientSpec::default(),
            sink: SignalSink::Fill(b),
        });
        rebuild_wires(&mut graph, &bindings);
        assert_eq!(graph.snarl.wires().count(), 1, "one wire for the binding");

        // The wire runs A's speed output (index 0) → B's fill input (index 0).
        let (out, in_) = graph.snarl.wires().next().unwrap();
        assert_eq!(out.output, 0, "speed is output 0");
        assert_eq!(in_.input, 0, "fill is input 0");
        assert_eq!(graph.keys[&GraphKey::Body(a)], out.node);
        assert_eq!(graph.keys[&GraphKey::Body(b)], in_.node);
    }

    #[test]
    fn source_and_sink_pins_match_the_port_order() {
        let id = StableId::new();
        // Sensor index must line up with body_sensors order.
        assert_eq!(sensor_index(&SignalSource::Speed(id)), Some(0));
        assert_eq!(sensor_index(&SignalSource::ContactCount(id)), Some(5));
        assert_eq!(sensor_index(&SignalSource::Named("x".into())), None);
        assert_eq!(sink_pin(&SignalSink::Fill(id)), (GraphKey::Body(id), 0));
        assert_eq!(
            sink_pin(&SignalSink::TracerColor(id)),
            (GraphKey::Body(id), 1)
        );
        assert_eq!(sink_pin(&SignalSink::Plot), (GraphKey::Scope, 0));
    }

    #[test]
    fn a_plot_binding_wires_into_the_scope_block() {
        let a = StableId::new();
        let mut graph = NodeGraph::default();
        body(&mut graph, a);
        reconcile_add(&mut graph, NodeData::Scope);
        let mut bindings = SignalBindings::default();
        bindings.0.push(SignalBinding {
            name: "speed@x".into(),
            source: SignalSource::Speed(a),
            map: SignalMap::default(),
            gradient: GradientSpec::default(),
            sink: SignalSink::Plot,
        });
        rebuild_wires(&mut graph, &bindings);
        assert_eq!(graph.snarl.wires().count(), 1, "plot binding → scope wire");
        let (_, in_) = graph.snarl.wires().next().unwrap();
        assert_eq!(
            graph.keys[&GraphKey::Scope],
            in_.node,
            "into the Scope block"
        );
    }

    #[test]
    fn input_values_reflect_bound_bus_signals() {
        let a = StableId::new();
        let b = StableId::new();
        let mut bus = SignalBus::default();
        bus.publish("wire", 42.0, false);
        let mut bindings = SignalBindings::default();
        bindings.0.push(SignalBinding {
            name: "wire".into(),
            source: SignalSource::Speed(a),
            map: SignalMap::default(),
            gradient: GradientSpec::default(),
            sink: SignalSink::Fill(b),
        });
        let values = binding_input_values(&bindings, &bus);
        // Body b's fill input (index 0) carries the "wire" value.
        assert_eq!(values[&GraphKey::Body(b)][0], Some(42.0));
    }

    #[test]
    fn fresh_param_name_avoids_collisions() {
        let mut params = SignalParams::default();
        assert_eq!(fresh_param_name(&params), "param-1");
        params.0.push(crate::signal::SignalParam::unit("param-1"));
        assert_eq!(fresh_param_name(&params), "param-2");
    }

    #[test]
    fn a_modulation_block_operand_wires_to_its_named_producer() {
        let mut graph = NodeGraph::default();
        // A param "amp" and a Gain block reading it.
        reconcile_add(&mut graph, NodeData::Param("amp".into()));
        reconcile_add(
            &mut graph,
            NodeData::Computed {
                name: "gain-1".into(),
                inputs: vec![],
                block: Some(BlockOp::Gain {
                    input: Some("amp".into()),
                    k: 2.0,
                }),
            },
        );
        rebuild_wires(&mut graph, &SignalBindings::default());
        assert_eq!(graph.snarl.wires().count(), 1, "operand wire drawn");
        let (out, in_) = graph.snarl.wires().next().unwrap();
        assert_eq!(graph.keys[&GraphKey::Param("amp".into())], out.node);
        assert_eq!(graph.keys[&GraphKey::Computed("gain-1".into())], in_.node);
        assert_eq!(in_.input, 0, "the gain's single input slot");
    }
}
