//! The **node-graph canvas**: the Simulink-style visual editor the signal
//! dataflow (`docs/signal-dataflow.md`) has been growing toward. It renders
//! the bus-named dataflow as draggable boxes with input/output ports and
//! bezier wires, and lets you **rewire an actuator** by dragging from a
//! producer's output port onto the actuator's input — a real graph edit,
//! routed through the same undoable `PropertyEditIntent` seam the dock uses.
//!
//! What appears on the canvas is exactly the bus graph:
//! - **params** (`defparam`) and **sensor nodes** are producers (one output
//!   port = the bus name they publish);
//! - **computed signals** (`defsignal`) are modulators (input ports = the
//!   names their expression reads, one output = the name they publish);
//! - **actuator nodes** are consumers (one input port = the signal they
//!   read; rewirable by drag).
//!
//! The wire protocol is the [`SignalBus`] name: a wire is drawn wherever a
//! consumer input's name matches a producer output's name — the graph is
//! *already* connected by naming, the canvas just makes it visible and lets
//! you re-point the endpoints. Layout is pure editor view-state (positions
//! in [`NodeGraph`], never persisted); the only authored write is the
//! actuator rewire, which goes through the command stack.

use crate::command::intent::PropertyEditIntent;
use crate::command::property::{PropertyChange, PropertyValue};
use crate::core::ids::StableId;
use crate::domain::node::{BehaviorNode, NodeKind};
use crate::signal::{ComputedSignals, SignalBus, SignalParams};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::collections::HashMap;

/// Identity of a box on the canvas — the node-graph analogue of a
/// [`StableId`], but spanning the config-seam producers (params, computed)
/// as well as the authored behavior nodes.
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
    /// The canvas x (relative to the canvas origin) for this role's column.
    fn column_x(self) -> f32 {
        match self {
            Self::Producer => 12.0,
            Self::Modulator => 180.0,
            Self::Consumer => 348.0,
        }
    }
}

/// One box on the canvas, assembled each frame from the live dataflow.
struct GraphNode {
    key: GraphKey,
    title: String,
    subtitle: String,
    /// Input-port bus names (consumer/modulator reads).
    inputs: Vec<String>,
    /// Output-port bus name (producer/modulator publishes), if any.
    output: Option<String>,
    role: Role,
    /// For an actuator: `(id, current kind)`, so a wire drop can emit the
    /// undoable rewire. `None` for read-only nodes.
    rewire: Option<(StableId, NodeKind)>,
}

/// The node-graph canvas state: visibility, box layout, and the in-flight
/// wire drag. Pure editor view-state — never authored, never persisted
/// (like a panel's scroll position).
#[derive(Resource, Default)]
pub struct NodeGraph {
    open: bool,
    positions: HashMap<GraphKey, egui::Pos2>,
    /// The producer-output bus name currently being dragged into a new wire.
    pending_wire: Option<String>,
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

const NODE_W: f32 = 130.0;
const HEADER_H: f32 = 30.0;
const PORT_ROW_H: f32 = 15.0;

/// A node box's height from its port count (one row per input, at least one).
fn node_height(inputs: usize) -> f32 {
    HEADER_H + inputs.max(1) as f32 * PORT_ROW_H
}

/// Fill color for a role's box.
fn role_fill(role: Role) -> egui::Color32 {
    match role {
        Role::Producer => egui::Color32::from_rgb(38, 66, 44),
        Role::Modulator => egui::Color32::from_rgb(38, 52, 74),
        Role::Consumer => egui::Color32::from_rgb(74, 52, 38),
    }
}

/// Renders the node-graph canvas (toolbar toggle). Draggable boxes + name
/// matched wires; dragging a producer output onto an actuator input rewires
/// it (undoable).
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

    let items = collect_nodes(&params, &computed, &bus, &nodes);

    let mut open = true;
    egui::Window::new("Node Graph")
        .open(&mut open)
        .default_width(520.0)
        .default_height(400.0)
        .show(ctx, |ui| {
            if items.is_empty() {
                ui.weak(
                    "Place sensor/actuator nodes, or add params/computed signals — \
                     they appear here wired by signal name. Drag a green output onto \
                     an actuator input to rewire it.",
                );
                return;
            }
            ui.weak("Drag boxes to arrange · drag an output port onto an actuator input to rewire");
            let canvas = ui
                .allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover())
                .rect;
            draw_canvas(ui, canvas, &mut graph, &items, &mut edits);
        });
    graph.open = open;
    Ok(())
}

/// Builds the canvas node list from the live dataflow (params, computed
/// signals, sensor/actuator nodes). Tracers are physical trails, not part of
/// the signal graph, so they are excluded.
fn collect_nodes(
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

/// The per-column running y-cursor, so freshly appearing nodes stack down
/// their column instead of overlapping.
#[derive(Default)]
struct ColumnCursor {
    producer: f32,
    modulator: f32,
    consumer: f32,
}

impl ColumnCursor {
    fn next(&mut self, role: Role, height: f32) -> egui::Pos2 {
        let slot = match role {
            Role::Producer => &mut self.producer,
            Role::Modulator => &mut self.modulator,
            Role::Consumer => &mut self.consumer,
        };
        let y = *slot + 12.0;
        *slot = y + height;
        egui::pos2(role.column_x(), y)
    }
}

/// A resolved port: its screen position and (for inputs) the owning node.
struct Port {
    pos: egui::Pos2,
    /// Owning node index (for input ports, to resolve rewire targets).
    node: usize,
}

/// Draws every box and wire and handles dragging (reposition + rewire).
fn draw_canvas(
    ui: &mut egui::Ui,
    canvas: egui::Rect,
    graph: &mut NodeGraph,
    items: &[GraphNode],
    edits: &mut MessageWriter<PropertyEditIntent>,
) {
    let origin = canvas.min;
    let painter = ui.painter_at(canvas);
    let mut cursor = ColumnCursor::default();

    // First lay every node out (assigning a slot to newcomers) and interact
    // with its body for repositioning.
    let mut rects: Vec<egui::Rect> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let height = node_height(item.inputs.len());
        let rel = *graph
            .positions
            .entry(item.key.clone())
            .or_insert_with(|| cursor.next(item.role, height));
        let rect = egui::Rect::from_min_size(origin + rel.to_vec2(), egui::vec2(NODE_W, height));
        let resp = ui.interact(rect, ui.id().with(("gn", i)), egui::Sense::drag());
        if resp.dragged()
            && let Some(pos) = graph.positions.get_mut(&item.key)
        {
            *pos += resp.drag_delta();
        }
        rects.push(rect);
    }

    // Resolve ports and draw boxes. `outputs` maps a bus name to its output
    // port position (the wire endpoints); `inputs` records every input port.
    let mut outputs: HashMap<String, egui::Pos2> = HashMap::new();
    let mut inputs: Vec<(String, Port)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let rect = rects[i];
        painter.rect_filled(rect, 5.0, role_fill(item.role));
        painter.rect_stroke(
            rect,
            5.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.min + egui::vec2(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            &item.title,
            egui::FontId::proportional(12.0),
            egui::Color32::from_gray(230),
        );
        if !item.subtitle.is_empty() {
            painter.text(
                rect.min + egui::vec2(8.0, 20.0),
                egui::Align2::LEFT_TOP,
                &item.subtitle,
                egui::FontId::monospace(9.0),
                egui::Color32::from_gray(170),
            );
        }
        for (row, name) in item.inputs.iter().enumerate() {
            let pos = egui::pos2(rect.left(), rect.top() + HEADER_H + row as f32 * PORT_ROW_H);
            painter.circle_filled(pos, 4.0, egui::Color32::from_rgb(210, 170, 120));
            inputs.push((name.clone(), Port { pos, node: i }));
        }
        if let Some(name) = &item.output {
            let pos = egui::pos2(rect.right(), rect.center().y);
            let port_rect = egui::Rect::from_center_size(pos, egui::vec2(12.0, 12.0));
            let resp = ui.interact(port_rect, ui.id().with(("out", i)), egui::Sense::drag());
            if resp.drag_started() {
                graph.pending_wire = Some(name.clone());
            }
            let hot = graph.pending_wire.as_deref() == Some(name.as_str());
            painter.circle_filled(
                pos,
                if hot { 6.0 } else { 4.0 },
                egui::Color32::from_rgb(140, 220, 150),
            );
            outputs.insert(name.clone(), pos);
        }
    }

    // Draw the name-matched wires: every input whose name has a producer.
    for (name, port) in &inputs {
        if let Some(from) = outputs.get(name) {
            draw_wire(&painter, *from, port.pos, egui::Color32::from_gray(150));
        }
    }

    // The in-flight wire follows the pointer; on release over an actuator
    // input, rewire it (undoable) to the dragged producer's name.
    if let Some(name) = graph.pending_wire.clone() {
        if let (Some(from), Some(ptr)) =
            (outputs.get(&name).copied(), ui.ctx().pointer_interact_pos())
        {
            draw_wire(&painter, from, ptr, egui::Color32::from_rgb(150, 220, 160));
        }
        if ui.ctx().input(|i| i.pointer.any_released()) {
            if let Some(ptr) = ui.ctx().pointer_interact_pos()
                && let Some((_, port)) = inputs.iter().find(|(_, p)| p.pos.distance(ptr) <= 9.0)
                && let Some((id, kind)) = &items[port.node].rewire
                && let Some(next) = rewired(kind, &name)
            {
                edits.write(PropertyEditIntent {
                    changes: vec![PropertyChange {
                        id: *id,
                        old: PropertyValue::NodeKind(kind.clone()),
                        new: PropertyValue::NodeKind(next),
                    }],
                });
            }
            graph.pending_wire = None;
        }
    }
}

/// A cubic bezier wire from an output port to an input port (horizontal
/// tangents, so wires bow like patch cables).
fn draw_wire(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32) {
    let dx = (to.x - from.x).abs().max(30.0) * 0.5;
    let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
        [
            from,
            from + egui::vec2(dx, 0.0),
            to - egui::vec2(dx, 0.0),
            to,
        ],
        false,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.5, color),
    );
    painter.add(bezier);
}

/// Returns `kind` with its input signal repointed to `signal`, or `None` if
/// it is not a rewirable consumer (only actuators rewire today).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::node::ActuatorTarget;
    use crate::domain::signal::{GradientSpec, SignalMap};

    #[test]
    fn node_height_grows_with_inputs() {
        assert!(
            (node_height(0) - (HEADER_H + PORT_ROW_H)).abs() < 1e-6,
            "at least one row"
        );
        assert!((node_height(3) - (HEADER_H + 3.0 * PORT_ROW_H)).abs() < 1e-6);
    }

    #[test]
    fn columns_order_left_to_right() {
        assert!(Role::Producer.column_x() < Role::Modulator.column_x());
        assert!(Role::Modulator.column_x() < Role::Consumer.column_x());
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
        // Non-consumers do not rewire.
        assert!(
            rewired(
                &NodeKind::Sensor {
                    quantity: crate::domain::node::SensorQuantity::Speed,
                    signal: "s".into()
                },
                "x"
            )
            .is_none()
        );
    }
}
