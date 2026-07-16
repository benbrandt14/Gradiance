//! The **node-graph canvas**: the Simulink-style visual editor for the signal
//! dataflow (`docs/signal-dataflow.md`), built on the [`egui-snarl`] node-graph
//! widget (the one major node editor tracking our pinned egui 0.35 —
//! `egui_node_graph2` is stuck on 0.29). snarl owns the box layout, pan/zoom,
//! and the drag-to-connect interaction; this module is the **adapter** between
//! it and our ECS-derived dataflow.
//!
//! The graph is *derived from the scene*, not owned by the canvas: every frame
//! `reconcile` rebuilds the snarl node set from the live dataflow (params,
//! computed signals, sensor/actuator nodes) keyed by `GraphKey`, preserving
//! each box's dragged position, and re-draws the wires by **bus-name match**
//! (a producer's output name to every consumer input reading it). The only
//! authored write is a **rewire**: dragging a producer output onto an actuator
//! input (or disconnecting one) emits an undoable `PropertyEditIntent` that
//! re-points the actuator's `signal` — snarl's own graph is never the source of
//! truth, so the wire follows once the command lands.
//!
//! [`egui-snarl`]: https://crates.io/crates/egui-snarl

use crate::command::intent::PropertyEditIntent;
use crate::command::property::{PropertyChange, PropertyValue};
use crate::core::ids::StableId;
use crate::domain::node::{BehaviorNode, NodeKind};
use crate::signal::{ComputedSignals, SignalBus, SignalParams};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer};
use egui_snarl::{InPin, InPinId, NodeId, OutPin, OutPinId, Snarl};
use std::collections::{HashMap, HashSet};

/// Identity of a box on the canvas — the node-graph analogue of a
/// [`StableId`], spanning config-seam producers (params, computed) and the
/// authored behavior nodes, so reconciliation can match a snarl node to its
/// dataflow source across frames.
#[derive(Clone, PartialEq, Eq, Hash)]
enum GraphKey {
    /// A `defparam` knob, by name.
    Param(String),
    /// A `defsignal` modulator, by name.
    Computed(String),
    /// An authored behavior node (sensor/actuator), by id.
    Node(StableId),
}

/// Which column a node sorts into (producers left, modulators middle,
/// consumers right) — the readable left-to-right dataflow.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// A source: params, sensors (output only).
    Producer,
    /// A modulator: computed signals (inputs + output).
    Modulator,
    /// A sink: actuators (input only).
    Consumer,
}

impl Role {
    /// The initial canvas x for this role's column (before the user drags).
    fn column_x(self) -> f32 {
        match self {
            Self::Producer => 20.0,
            Self::Modulator => 220.0,
            Self::Consumer => 420.0,
        }
    }

    /// Pin/accent color, shared with the scene-view node glyphs.
    fn color(self) -> egui::Color32 {
        match self {
            Self::Producer => egui::Color32::from_rgb(90, 200, 120),
            Self::Modulator => egui::Color32::from_rgb(120, 170, 235),
            Self::Consumer => egui::Color32::from_rgb(235, 165, 100),
        }
    }
}

/// One box on the canvas, owned by the [`Snarl`] but rebuilt each frame from
/// the live dataflow by [`reconcile`].
#[derive(Clone)]
struct GraphNode {
    key: GraphKey,
    title: String,
    subtitle: String,
    /// Input-port bus names (consumer/modulator reads).
    inputs: Vec<String>,
    /// Output-port bus name (producer/modulator publishes), if any.
    output: Option<String>,
    role: Role,
    /// For an actuator: `(id, current kind)`, so a wire connect/disconnect
    /// can emit the undoable rewire. `None` for read-only nodes.
    rewire: Option<(StableId, NodeKind)>,
}

/// The node-graph canvas state: visibility plus the snarl graph and the
/// `GraphKey` → `NodeId` map reconciliation keeps in sync. Pure editor
/// view-state — never authored, never persisted (like a scroll position).
#[derive(Resource)]
pub struct NodeGraph {
    open: bool,
    snarl: Snarl<GraphNode>,
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

/// Renders the node-graph canvas (toolbar toggle). Reconciles the snarl graph
/// from the live dataflow, shows it, and drains any rewire the user made into
/// undoable intents.
pub fn node_graph_panel(
    mut contexts: EguiContexts,
    mut graph: ResMut<NodeGraph>,
    params: Res<SignalParams>,
    computed: Res<ComputedSignals>,
    bus: Res<SignalBus>,
    nodes: Query<(&StableId, &NodeKind), With<BehaviorNode>>,
    mut edits: MessageWriter<PropertyEditIntent>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !graph.open {
        return Ok(());
    }

    let desired = collect_desired(&params, &computed, &bus, &nodes);
    reconcile(&mut graph, desired);

    let mut viewer = GraphViewer::default();
    let style = SnarlStyle::new();
    let mut open = true;
    egui::Window::new("Node Graph")
        .open(&mut open)
        .default_width(560.0)
        .default_height(420.0)
        .show(ctx, |ui| {
            if graph.keys.is_empty() {
                ui.weak(
                    "Place sensor/actuator nodes, or add params/computed signals — \
                     they appear here wired by signal name. Drag a producer output \
                     onto an actuator input to rewire it.",
                );
                return;
            }
            graph.snarl.show(&mut viewer, &style, "node-graph", ui);
        });
    graph.open = open;

    for change in viewer.rewires {
        edits.write(PropertyEditIntent {
            changes: vec![change],
        });
    }
    Ok(())
}

/// Builds the desired canvas node set from the live dataflow (params, computed
/// signals, sensor/actuator nodes). Tracers are physical trails, not part of
/// the signal graph, so they are excluded.
fn collect_desired(
    params: &SignalParams,
    computed: &ComputedSignals,
    bus: &SignalBus,
    nodes: &Query<(&StableId, &NodeKind), With<BehaviorNode>>,
) -> Vec<GraphNode> {
    let mut items = Vec::new();
    for param in &params.0 {
        items.push(GraphNode {
            key: GraphKey::Param(param.name.clone()),
            title: format!("⊙ {}", param.name),
            subtitle: format!("{:.3}", param.value),
            inputs: Vec::new(),
            output: Some(param.name.clone()),
            role: Role::Producer,
            rewire: None,
        });
    }
    for (id, kind) in nodes {
        match kind {
            NodeKind::Sensor { quantity, signal } => items.push(GraphNode {
                key: GraphKey::Node(*id),
                title: "▷ sensor".into(),
                subtitle: quantity.label().into(),
                inputs: Vec::new(),
                output: Some(signal.clone()),
                role: Role::Producer,
                rewire: None,
            }),
            NodeKind::Actuator {
                signal,
                gradient,
                target,
                ..
            } => items.push(GraphNode {
                key: GraphKey::Node(*id),
                title: "◁ actuator".into(),
                subtitle: format!("{gradient:?} → {}", target.label()),
                inputs: vec![signal.clone()],
                output: None,
                role: Role::Consumer,
                rewire: Some((*id, kind.clone())),
            }),
            NodeKind::Tracer(_) => {}
        }
    }
    for signal in &computed.0 {
        let value = bus
            .get(&signal.name)
            .map_or_else(String::new, |v| format!("{v:.3}"));
        items.push(GraphNode {
            key: GraphKey::Computed(signal.name.clone()),
            title: format!("ƒ {}", signal.name),
            subtitle: value,
            inputs: signal.expr.inputs(),
            output: Some(signal.name.clone()),
            role: Role::Modulator,
            rewire: None,
        });
    }
    items
}

/// The initial position for a freshly appearing node (before the user drags
/// it): its role's column, staggered down.
fn auto_pos(role: Role, index: usize) -> egui::Pos2 {
    egui::pos2(role.column_x(), 20.0 + (index % 8) as f32 * 72.0)
}

/// Rebuilds the snarl graph to mirror `desired`: updates existing boxes in
/// place (keeping their dragged position), inserts new ones, removes vanished
/// ones, then re-draws the wires by bus-name match. This makes the canvas a
/// *view* of the ECS dataflow — the scene is the source of truth.
fn reconcile(graph: &mut NodeGraph, desired: Vec<GraphNode>) {
    let mut seen: HashSet<GraphKey> = HashSet::with_capacity(desired.len());
    for node in desired {
        let key = node.key.clone();
        seen.insert(key.clone());
        if let Some(&id) = graph.keys.get(&key)
            && let Some(slot) = graph.snarl.get_node_mut(id)
        {
            *slot = node;
            continue;
        }
        let pos = auto_pos(node.role, graph.keys.len());
        let id = graph.snarl.insert_node(pos, node);
        graph.keys.insert(key, id);
    }
    // Drop nodes whose dataflow source is gone.
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
    rebuild_wires(&mut graph.snarl);
}

/// Clears every wire and re-draws it from bus-name matching: a producer's
/// output pin to each consumer/modulator input pin reading the same name. The
/// naming *is* the connection; the canvas just visualizes it.
fn rebuild_wires(snarl: &mut Snarl<GraphNode>) {
    let existing: Vec<(OutPinId, InPinId)> = snarl.wires().collect();
    for (out, in_) in existing {
        snarl.disconnect(out, in_);
    }
    // Map each output bus name to its producing node.
    let outputs: HashMap<String, NodeId> = snarl
        .node_ids()
        .filter_map(|(id, node)| node.output.clone().map(|name| (name, id)))
        .collect();
    // For each input pin, connect the matching producer (if any).
    let links: Vec<(OutPinId, InPinId)> = snarl
        .node_ids()
        .flat_map(|(id, node)| {
            node.inputs
                .iter()
                .enumerate()
                .filter_map(|(input, name)| {
                    outputs.get(name).map(|&src| {
                        (
                            OutPinId {
                                node: src,
                                output: 0,
                            },
                            InPinId { node: id, input },
                        )
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for (out, in_) in links {
        snarl.connect(out, in_);
    }
}

/// Returns `kind` with its input signal repointed to `signal`, or `None` if it
/// is not a rewirable consumer (only actuators rewire today).
fn rewired(kind: &NodeKind, signal: &str) -> Option<NodeKind> {
    match kind {
        NodeKind::Actuator {
            map,
            gradient,
            target,
            ..
        } => Some(NodeKind::Actuator {
            signal: signal.to_owned(),
            map: *map,
            gradient: *gradient,
            target: *target,
        }),
        _ => None,
    }
}

/// The snarl viewer: renders each box's title/subtitle/pins and translates
/// user connect/disconnect gestures into rewire [`PropertyChange`]s (drained
/// by the system into undoable intents). It never mutates the snarl graph —
/// reconciliation owns the wires.
#[derive(Default)]
struct GraphViewer {
    /// Rewires the user made this frame (actuator signal edits).
    rewires: Vec<PropertyChange>,
}

impl SnarlViewer<GraphNode> for GraphViewer {
    fn title(&mut self, node: &GraphNode) -> String {
        node.title.clone()
    }

    fn inputs(&mut self, node: &GraphNode) -> usize {
        node.inputs.len()
    }

    fn outputs(&mut self, node: &GraphNode) -> usize {
        usize::from(node.output.is_some())
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<GraphNode>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let (label, color) = snarl.get_node(pin.id.node).map_or_else(
            || (String::new(), egui::Color32::GRAY),
            |node| {
                let name = node.inputs.get(pin.id.input).cloned().unwrap_or_default();
                (name, node.role.color())
            },
        );
        ui.label(if label.is_empty() {
            "(unwired)".to_owned()
        } else {
            label
        });
        PinInfo::circle().with_fill(color)
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<GraphNode>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let (label, subtitle, color) = snarl.get_node(pin.id.node).map_or_else(
            || (String::new(), String::new(), egui::Color32::GRAY),
            |node| {
                (
                    node.output.clone().unwrap_or_default(),
                    node.subtitle.clone(),
                    node.role.color(),
                )
            },
        );
        ui.vertical(|ui| {
            ui.label(label);
            if !subtitle.is_empty() {
                ui.weak(subtitle);
            }
        });
        PinInfo::circle().with_fill(color)
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<GraphNode>) {
        // A producer output → an actuator input: rewire the actuator's signal
        // to the producer's bus name (undoable). Do not touch the snarl graph
        // — reconciliation redraws the wire once the command lands.
        let Some(name) = snarl
            .get_node(from.id.node)
            .and_then(|node| node.output.clone())
        else {
            return;
        };
        self.push_rewire(snarl.get_node(to.id.node), &name);
    }

    fn disconnect(&mut self, _from: &OutPin, to: &InPin, snarl: &mut Snarl<GraphNode>) {
        // Detach: clear the actuator's signal (an empty wire name).
        self.push_rewire(snarl.get_node(to.id.node), "");
    }

    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<GraphNode>) {
        self.push_rewire(snarl.get_node(pin.id.node), "");
    }
}

impl GraphViewer {
    /// Queues an undoable rewire of `node` (if it is a rewirable actuator) to
    /// read `signal`.
    fn push_rewire(&mut self, node: Option<&GraphNode>, signal: &str) {
        if let Some((id, old)) = node.and_then(|n| n.rewire.as_ref())
            && let Some(new) = rewired(old, signal)
        {
            self.rewires.push(PropertyChange {
                id: *id,
                old: PropertyValue::NodeKind(old.clone()),
                new: PropertyValue::NodeKind(new),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::node::{ActuatorTarget, SensorQuantity};
    use crate::domain::signal::{GradientSpec, SignalMap};

    fn sensor(signal: &str) -> GraphNode {
        GraphNode {
            key: GraphKey::Node(StableId::new()),
            title: "sensor".into(),
            subtitle: String::new(),
            inputs: Vec::new(),
            output: Some(signal.into()),
            role: Role::Producer,
            rewire: None,
        }
    }

    fn actuator(id: StableId, signal: &str) -> GraphNode {
        let kind = NodeKind::Actuator {
            signal: signal.into(),
            map: SignalMap::default(),
            gradient: GradientSpec::Turbo,
            target: ActuatorTarget::Fill,
        };
        GraphNode {
            key: GraphKey::Node(id),
            title: "actuator".into(),
            subtitle: String::new(),
            inputs: vec![signal.into()],
            output: None,
            role: Role::Consumer,
            rewire: Some((id, kind)),
        }
    }

    #[test]
    fn reconcile_matches_wires_by_name_and_prunes_vanished_nodes() {
        let mut graph = NodeGraph::default();
        let act_id = StableId::new();
        // A sensor publishing "wire" and an actuator reading "wire" → one wire.
        reconcile(&mut graph, vec![sensor("wire"), actuator(act_id, "wire")]);
        assert_eq!(graph.keys.len(), 2);
        assert_eq!(graph.snarl.wires().count(), 1, "name match draws one wire");

        // Rename the actuator to read a non-existent signal → no wire.
        reconcile(&mut graph, vec![sensor("wire"), actuator(act_id, "other")]);
        assert_eq!(graph.snarl.wires().count(), 0, "no producer for 'other'");

        // Drop the sensor → only the actuator remains, still no wire.
        reconcile(&mut graph, vec![actuator(act_id, "wire")]);
        assert_eq!(graph.keys.len(), 1, "vanished node pruned");
        assert_eq!(graph.snarl.wires().count(), 0);
    }

    #[test]
    fn connecting_a_producer_to_an_actuator_queues_a_rewire() {
        let mut graph = NodeGraph::default();
        let act_id = StableId::new();
        reconcile(&mut graph, vec![sensor("wire"), actuator(act_id, "old")]);

        // Find the two node ids.
        let ids: Vec<NodeId> = graph.snarl.node_ids().map(|(id, _)| id).collect();
        let (sensor_id, actuator_id) =
            graph
                .snarl
                .node_ids()
                .fold((ids[0], ids[0]), |(s, a), (id, node)| {
                    if node.output.is_some() {
                        (id, a)
                    } else {
                        (s, id)
                    }
                });

        let mut viewer = GraphViewer::default();
        let from = graph.snarl.out_pin(OutPinId {
            node: sensor_id,
            output: 0,
        });
        let to = graph.snarl.in_pin(InPinId {
            node: actuator_id,
            input: 0,
        });
        viewer.connect(&from, &to, &mut graph.snarl);

        assert_eq!(viewer.rewires.len(), 1, "one rewire queued");
        match &viewer.rewires[0].new {
            PropertyValue::NodeKind(NodeKind::Actuator { signal, .. }) => {
                assert_eq!(signal, "wire", "actuator repointed to the sensor's name");
            }
            other => panic!("expected an actuator rewire, got {other:?}"),
        }
    }

    #[test]
    fn rewiring_an_actuator_repoints_only_its_signal() {
        let kind = NodeKind::Actuator {
            signal: "old".into(),
            map: SignalMap::default(),
            gradient: GradientSpec::Turbo,
            target: ActuatorTarget::TracerColor,
        };
        let next = rewired(&kind, "new").unwrap();
        assert_eq!(
            next,
            NodeKind::Actuator {
                signal: "new".into(),
                map: SignalMap::default(),
                gradient: GradientSpec::Turbo,
                target: ActuatorTarget::TracerColor,
            }
        );
        assert!(
            rewired(
                &NodeKind::Sensor {
                    quantity: SensorQuantity::Speed,
                    signal: "s".into()
                },
                "x"
            )
            .is_none()
        );
    }
}
