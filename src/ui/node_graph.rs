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
//! body that participates in a binding or is selected, plus every param and
//! computed signal, keyed by `GraphKey` so dragged positions persist; the
//! wires are rebuilt from the bindings.
//!
//! [`egui-snarl`]: https://crates.io/crates/egui-snarl
//! [`SignalBinding`]: crate::domain::signal::SignalBinding

use crate::core::ids::StableId;
use crate::domain::Body;
use crate::interaction::selection::Selection;
use crate::signal::{
    ComputedSignals, GradientSpec, SignalBinding, SignalBindings, SignalMap, SignalParams,
    SignalSink, SignalSource,
};
use crate::ui::ports::{body_actuators, body_sensors};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer};
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
}

/// What a block *is* — carries what the viewer needs to label pins and resolve
/// a pin back to a [`SignalSource`] / [`SignalSink`].
#[derive(Clone)]
enum NodeData {
    /// A body block: outputs are `body_sensors`, inputs are `body_actuators`.
    Body(StableId),
    /// A param block: one output (its published bus name).
    Param(String),
    /// A computed block: inputs are the bus names its expression reads, one
    /// output (its published name).
    Computed { name: String, inputs: Vec<String> },
}

impl NodeData {
    fn key(&self) -> GraphKey {
        match self {
            Self::Body(id) => GraphKey::Body(*id),
            Self::Param(name) => GraphKey::Param(name.clone()),
            Self::Computed { name, .. } => GraphKey::Computed(name.clone()),
        }
    }

    fn role(&self) -> Role {
        match self {
            Self::Body(_) => Role::Body,
            Self::Param(_) => Role::Producer,
            Self::Computed { .. } => Role::Modulator,
        }
    }

    /// The [`SignalSource`] a given output pin publishes (for wiring).
    fn output_source(&self, index: usize) -> Option<SignalSource> {
        match self {
            Self::Body(id) => body_sensors(*id).get(index).map(|(_, s)| s.clone()),
            Self::Param(name) => (index == 0).then(|| SignalSource::Named(name.clone())),
            Self::Computed { name, .. } => (index == 0).then(|| SignalSource::Named(name.clone())),
        }
    }

    /// The [`SignalSink`] a given input pin drives (only body actuators are
    /// wireable targets; a computed input is read-only display).
    fn input_sink(&self, index: usize) -> Option<SignalSink> {
        match self {
            Self::Body(id) => body_actuators(*id).get(index).map(|(_, s)| s.clone()),
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
}

impl Role {
    fn column_x(self) -> f32 {
        match self {
            Self::Producer => 20.0,
            Self::Modulator => 220.0,
            Self::Body => 420.0,
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
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            open: false,
            snarl: Snarl::new(),
            keys: HashMap::new(),
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

/// The `(GraphKey, input index)` a binding sink resolves to (body actuators;
/// `Plot` has no block).
fn sink_pin(sink: &SignalSink) -> Option<(GraphKey, usize)> {
    match sink {
        SignalSink::Fill(id) => Some((GraphKey::Body(*id), 0)),
        SignalSink::TracerColor(id) => Some((GraphKey::Body(*id), 1)),
        SignalSink::Plot => None,
    }
}

/// Renders the node-graph canvas. Reconciles blocks + wires from the live
/// dataflow, shows it, and applies any wire the user made/removed to the
/// bindings (config-seam direct edit, like the Signals dock).
pub fn node_graph_panel(
    mut contexts: EguiContexts,
    mut graph: ResMut<NodeGraph>,
    mut bindings: ResMut<SignalBindings>,
    params: Res<SignalParams>,
    computed: Res<ComputedSignals>,
    bodies: Query<&StableId, With<Body>>,
    selection: Res<Selection>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !graph.open {
        return Ok(());
    }

    let desired = collect_desired(&bindings, &params, &computed, &bodies, &selection);
    reconcile(&mut graph, desired);
    rebuild_wires(&mut graph, &bindings);

    let mut viewer = GraphViewer::default();
    let style = SnarlStyle::new();
    let mut open = true;
    egui::Window::new("Node Graph")
        .open(&mut open)
        .default_width(620.0)
        .default_height(440.0)
        .show(ctx, |ui| {
            if graph.keys.is_empty() {
                ui.weak(
                    "Select a body (or add a param/computed signal) to place it here, \
                     then drag a sensor output onto another body's actuator input to \
                     wire them.",
                );
                return;
            }
            ui.weak("Drag a sensor output → a body's actuator input to bind · drag off to unbind");
            graph.snarl.show(&mut viewer, &style, "node-graph", ui);
        });
    graph.open = open;

    // Apply wiring edits (config-seam: bindings are edited directly).
    for binding in viewer.new_bindings {
        bindings.0.push(binding);
    }
    for (source, sink) in viewer.removed {
        bindings
            .0
            .retain(|b| !(b.source == source && b.sink == sink));
    }
    Ok(())
}

/// Builds the desired block set: a node per body referenced by a binding or
/// currently selected, plus every param and computed signal.
fn collect_desired(
    bindings: &SignalBindings,
    params: &SignalParams,
    computed: &ComputedSignals,
    bodies: &Query<&StableId, With<Body>>,
    selection: &Selection,
) -> Vec<NodeData> {
    let live: HashSet<StableId> = bodies.iter().copied().collect();
    let mut shown: HashSet<StableId> = HashSet::new();
    // Bodies referenced by a binding.
    for binding in &bindings.0 {
        for id in binding_bodies(binding) {
            if live.contains(&id) {
                shown.insert(id);
            }
        }
    }
    // Selected bodies.
    for entity in selection.iter() {
        if let Ok(id) = bodies.get(entity) {
            shown.insert(*id);
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
        });
    }
    for id in shown {
        items.push(NodeData::Body(id));
    }
    items
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
}

impl ColumnCursor {
    fn next(&mut self, role: Role) -> egui::Pos2 {
        let slot = match role {
            Role::Producer => &mut self.producer,
            Role::Modulator => &mut self.modulator,
            Role::Body => &mut self.body,
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
    new_bindings: Vec<SignalBinding>,
    removed: Vec<(SignalSource, SignalSink)>,
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
}

impl SnarlViewer<NodeData> for GraphViewer {
    fn title(&mut self, node: &NodeData) -> String {
        match node {
            NodeData::Body(id) => format!("◻ body {id:.4}"),
            NodeData::Param(name) => format!("⊙ {name}"),
            NodeData::Computed { name, .. } => format!("ƒ {name}"),
        }
    }

    fn inputs(&mut self, node: &NodeData) -> usize {
        match node {
            NodeData::Body(id) => body_actuators(*id).len(),
            NodeData::Param(_) => 0,
            NodeData::Computed { inputs, .. } => inputs.len(),
        }
    }

    fn outputs(&mut self, node: &NodeData) -> usize {
        match node {
            NodeData::Body(id) => body_sensors(*id).len(),
            NodeData::Param(_) | NodeData::Computed { .. } => 1,
        }
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let label = match snarl.get_node(pin.id.node) {
            Some(NodeData::Body(id)) => body_actuators(*id)
                .get(pin.id.input)
                .map_or_else(String::new, |(l, _)| (*l).to_owned()),
            Some(NodeData::Computed { inputs, .. }) => {
                inputs.get(pin.id.input).cloned().unwrap_or_default()
            }
            _ => String::new(),
        };
        ui.label(label);
        PinInfo::circle().with_fill(egui::Color32::from_rgb(210, 170, 120))
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let label = match snarl.get_node(pin.id.node) {
            Some(NodeData::Body(id)) => body_sensors(*id)
                .get(pin.id.output)
                .map_or_else(String::new, |(l, _)| (*l).to_owned()),
            Some(NodeData::Param(name) | NodeData::Computed { name, .. }) => name.clone(),
            None => String::new(),
        };
        ui.label(label);
        PinInfo::circle().with_fill(egui::Color32::from_rgb(140, 220, 150))
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeData>) {
        // A producer output → a body actuator input: create a binding
        // (source → sink). Don't mutate snarl; reconciliation redraws the wire
        // once the binding lands.
        if let (Some(source), Some(sink)) = (
            Self::out_source(snarl, from.id),
            Self::in_sink(snarl, to.id),
        ) {
            self.new_bindings.push(SignalBinding {
                name: format!("wire-{}", self.new_bindings.len() + 1),
                source,
                map: SignalMap::default(),
                gradient: GradientSpec::default(),
                sink,
            });
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeData>) {
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
        let (Some((src_key, out)), Some((sink_key, in_))) =
            (source_pin(&binding.source), sink_pin(&binding.sink))
        else {
            continue;
        };
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(graph: &mut NodeGraph, id: StableId) {
        reconcile_add(graph, NodeData::Body(id));
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
        assert_eq!(
            sink_pin(&SignalSink::Fill(id)),
            Some((GraphKey::Body(id), 0))
        );
        assert_eq!(
            sink_pin(&SignalSink::TracerColor(id)),
            Some((GraphKey::Body(id), 1))
        );
        assert_eq!(sink_pin(&SignalSink::Plot), None);
    }
}
