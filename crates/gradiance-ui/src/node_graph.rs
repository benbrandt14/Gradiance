//! The **node-graph canvas**: the Simulink-style visual editor for the signal
//! dataflow (`docs/signal-dataflow.md`), built on the [`egui-snarl`] node-graph
//! widget (the one major node editor tracking our pinned egui 0.36 —
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
//! — never merely selected, so the canvas stays uncluttered — plus every signal
//! node of the authored [`Graph`] (params, blocks, keys, `defsignal`
//! expressions), keyed by `GraphKey`; the wires are rebuilt from the bindings
//! and the graph's wires. It docks to the screen bottom.
//!
//! **The signal graph is the source** (brief G-2): a param or block is keyed
//! by its node id, sits at its saved [`Node::pos`](gradiance_graph::Node), and
//! a settled drag writes the new position back (one `settings` undo step, like
//! a slider drag). Every canvas edit of it is a [`GraphEdit`], applied through
//! [`Graph::edit`] (validated, so a wire the graph refuses never lands). The
//! canvas still writes the resource directly; G-3 moves it onto operations.
//!
//! [`egui-snarl`]: https://crates.io/crates/egui-snarl
//! [`SignalBinding`]: gradiance_domain::signal::SignalBinding

use crate::ports::{
    body_actuators, body_sensors, joint_actuators, joint_sensors, thruster_actuators,
};
use crate::widgets;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use egui_snarl::ui::{BackgroundPattern, PinInfo, SnarlStyle, SnarlViewer, WireStyle};
use egui_snarl::{InPin, InPinId, NodeId, OutPin, OutPinId, Snarl};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance_domain::quantity::Subject;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::signal::Subjects;
use gradiance_domain::{Body, Joint};
use gradiance_interaction::graph_keys::EditorKeyTable;
use gradiance_interaction::selection::Selection;
use gradiance_physics::queries::PhysicsQueries;
use gradiance_graph::{
    Family, GraphEdit, Input, Node as GraphNode, NodeKind as SignalKind, NodeRef, PortRef,
};
use gradiance_signal::{
    GradientSpec, Graph, SignalBinding, SignalBindings, SignalBus, SignalMap, SignalSink,
    SignalSource, read_source,
};
use std::collections::{HashMap, HashSet};

/// Identity of a block on the canvas, so reconciliation matches a snarl node
/// to its dataflow source across frames.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum GraphKey {
    /// A scene body (its sensor/actuator ports), by id.
    Body(StableId),
    /// A scene joint (its sensor ports, and its motor's actuator ports), by
    /// id (brief N-1).
    Joint(StableId),
    /// A behavior node (a thruster's thrust pin), by id (brief N-3).
    Node(StableId),
    /// A param node of the signal graph, by node id.
    Param(StableId),
    /// Any other signal node (a block, a key, a `defsignal`), by node id.
    Computed(StableId),
    /// The singleton **Scope** sink — the Live Plot as a block. Any signal
    /// wired into it becomes a plot-sink binding.
    Scope,
    /// A free-floating annotation, by its serial number.
    Comment(u32),
}

/// What a block *is* — carries what the viewer needs to label pins and resolve
/// a pin back to a [`SignalSource`] / [`SignalSink`].
#[derive(Clone)]
enum NodeData {
    /// A body block: outputs are `body_sensors`, inputs are `body_actuators`.
    /// `name` is an optional custom short label; without one the block shows
    /// its shape `kind` (box / circle / polygon …) so bodies are distinct.
    Body {
        id: StableId,
        name: Option<String>,
        kind: &'static str,
    },
    /// A joint block: outputs are `joint_sensors`; a motorised hinge or
    /// slider has `joint_actuators` inputs (motor speed, motor cap). `kind`
    /// is the label ("hinge", "slider", "spring"); `linear` picks the motor
    /// units (a slider's m/s and N).
    Joint {
        id: StableId,
        kind: &'static str,
        motor: bool,
        linear: bool,
    },
    /// A thruster block (brief N-3): one input, its thrust (N), and no
    /// outputs. Shown while a wire names it or its body is on the canvas.
    Thruster { id: StableId },
    /// A param node: one output (its published bus name), its value in the
    /// footer.
    Param(GraphNode),
    /// Any other signal node: its operand ports ([`operand_ports`]) are
    /// wireable pins, its other ports (`k`, `amp`, a curve) footer fields.
    Computed(GraphNode),
    /// The singleton **Scope** block: one input, no output — wiring a signal
    /// into it makes a `SignalSink::Plot` binding (the Live Plot as a block).
    Scope,
    /// A **comment**: free text, no pins, wired to nothing.
    ///
    /// Unlike every other block, a comment has no backing resource — it exists
    /// only because someone typed it. It is therefore pure view state, held in
    /// [`NodeGraph::comments`] and **not persisted**: the canvas layout is in
    /// flux, and inventing a file format for it now would mean migrating that
    /// format later. Comments survive reconcile, not restart.
    Comment {
        /// Serial number, the reconcile key (text changes, identity does not).
        id: u32,
        /// The note.
        text: String,
    },
}

impl NodeData {
    fn key(&self) -> GraphKey {
        match self {
            Self::Body { id, .. } => GraphKey::Body(*id),
            Self::Joint { id, .. } => GraphKey::Joint(*id),
            Self::Thruster { id } => GraphKey::Node(*id),
            Self::Param(node) => GraphKey::Param(node.id),
            Self::Computed(node) => GraphKey::Computed(node.id),
            Self::Scope => GraphKey::Scope,
            Self::Comment { id, .. } => GraphKey::Comment(*id),
        }
    }

    fn role(&self) -> Role {
        match self {
            Self::Body { .. } | Self::Joint { .. } | Self::Thruster { .. } => Role::Body,
            Self::Param(_) => Role::Producer,
            Self::Computed(_) => Role::Modulator,
            Self::Scope => Role::Scope,
            Self::Comment { .. } => Role::Comment,
        }
    }

    /// The short title — the block's *type*, or its custom name if set.
    fn title(&self) -> String {
        match self {
            Self::Body { name, kind, .. } => name.clone().unwrap_or_else(|| (*kind).to_owned()),
            Self::Joint { kind, .. } => (*kind).to_owned(),
            Self::Thruster { .. } => "thruster".to_owned(),
            Self::Param(node) => format!("⊙ {}", node_title(node)),
            Self::Computed(node) => format!("ƒ {}", node_title(node)),
            Self::Scope => "▭ Scope".to_owned(),
            Self::Comment { .. } => "note".to_owned(),
        }
    }

    /// The [`SignalSource`] a given output pin publishes (for wiring).
    fn output_source(&self, index: usize) -> Option<SignalSource> {
        match self {
            Self::Body { id, .. } => body_sensors(*id).get(index).map(|(_, s)| s.clone()),
            Self::Joint { id, .. } => joint_sensors(*id).get(index).map(|(_, s)| s.clone()),
            // A binding reads a node by its label; an unlabelled node
            // publishes nothing a binding could read.
            Self::Param(node) | Self::Computed(node) => (index == 0)
                .then(|| node.label.clone().map(SignalSource::Named))
                .flatten(),
            Self::Thruster { .. } | Self::Scope | Self::Comment { .. } => None,
        }
    }

    /// The [`SignalSink`] a given input pin drives.
    fn input_sink(&self, index: usize) -> Option<SignalSink> {
        match self {
            Self::Body { id, .. } => body_actuators(*id).get(index).map(|(_, s)| s.clone()),
            Self::Joint {
                id, motor: true, ..
            } => joint_actuators(*id).get(index).map(|(_, s)| s.clone()),
            Self::Thruster { id } => thruster_actuators(*id).get(index).map(|(_, s)| s.clone()),
            Self::Scope => (index == 0).then_some(SignalSink::Plot),
            _ => None,
        }
    }
}

impl NodeData {
    /// The signal-graph node behind a param or block.
    fn signal_node(&self) -> Option<&GraphNode> {
        match self {
            Self::Param(node) | Self::Computed(node) => Some(node),
            _ => None,
        }
    }

    /// Where the graph saves this block, for a signal node.
    fn saved_pos(&self) -> Option<egui::Pos2> {
        self.signal_node().map(|n| egui::pos2(n.pos.x, n.pos.y))
    }
}

/// A signal node's title: its label, or its kind's catalog name.
fn node_title(node: &GraphNode) -> String {
    node.label
        .clone()
        .unwrap_or_else(|| gradiance_graph::meta(&node.kind).name.to_owned())
}

/// The input ports of a signal kind the canvas shows as wireable pins (pin
/// `i` is port `operand_ports(kind)[i]`): the operands, as before G-2. The
/// other ports (a gain's `k`, an oscillator's `amp` and `freq`, a constant's
/// `value`, a curve) are edited as constants in the block's footer.
fn operand_ports(kind: &SignalKind) -> Vec<usize> {
    match kind {
        SignalKind::Gain | SignalKind::Abs | SignalKind::Curve => vec![0],
        SignalKind::Sum
        | SignalKind::Product
        | SignalKind::Sub
        | SignalKind::Min
        | SignalKind::Max => vec![0, 1],
        SignalKind::Expr(_) => (0..kind.input_ports().len()).collect(),
        _ => Vec::new(),
    }
}

/// Column/color role of a block.
#[derive(Clone, Copy)]
enum Role {
    Producer,
    Modulator,
    Body,
    Scope,
    /// Comments get their own column so a new one never lands on top of a
    /// block, and so the notes read as a margin rather than as the graph.
    Comment,
}

impl Role {
    fn column_x(self) -> f32 {
        match self {
            Self::Producer => 20.0,
            Self::Modulator => 220.0,
            Self::Body => 420.0,
            Self::Scope => 640.0,
            Self::Comment => 840.0,
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
    /// Free-floating annotations: `(serial, text)`. Pure view state — see
    /// [`NodeData::Comment`] for why they are not persisted.
    comments: Vec<(u32, String)>,
    /// Next comment serial. Monotonic, so deleting a note never lets a new one
    /// inherit its identity (and its dragged position).
    next_comment: u32,
    /// Set by the header **fit** button; consumed next frame to re-center the
    /// view on the graph's bounding box.
    fit_requested: bool,
    /// Last frame's pan/zoom transform (captured via `current_transform`), used
    /// to paint the canvas-following dot-grid a frame later.
    last_transform: Option<egui::emath::TSTransform>,
    /// The saved position each signal block was last placed at. A block moves
    /// to its saved position only when that changes (an undo, a
    /// `graph-set`), so a drag in progress is never snapped back.
    placed: HashMap<GraphKey, egui::Pos2>,
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            open: false,
            snarl: Snarl::new(),
            keys: HashMap::new(),
            added: HashSet::new(),
            names: HashMap::new(),
            comments: Vec::new(),
            next_comment: 0,
            fit_requested: false,
            last_transform: None,
            placed: HashMap::new(),
        }
    }
}

crate::impl_panel_toggle!(NodeGraph, open);

impl NodeGraph {
    /// Adds a body's block to the canvas and opens it (the "Add to node
    /// editor" context action).
    pub fn add_body(&mut self, id: StableId) {
        self.added.insert(id);
        self.open = true;
    }
}

/// The block key of signal node `id`: a param or any other signal node.
fn signal_key(graph: &Graph, id: StableId) -> Option<GraphKey> {
    let node = graph.node(id)?;
    match node.kind {
        SignalKind::Param { .. } => Some(GraphKey::Param(id)),
        _ if node.kind.family() == Family::Signal => Some(GraphKey::Computed(id)),
        _ => None,
    }
}

/// The `(GraphKey, output index)` a binding source resolves to, if it maps to
/// a producer block on the canvas: the signal node labelled with a named
/// source, a body sensor in [`body_sensors`] order, a joint sensor in
/// [`joint_sensors`] order. `None` for a two-body source (distance), which
/// has no block, or a name no node publishes.
fn source_pin(source: &SignalSource, graph: &Graph) -> Option<(GraphKey, usize)> {
    match (source, source.reading()) {
        (SignalSource::Named(name), _) => Some((signal_key(graph, graph.by_label(name)?.id)?, 0)),
        (_, Some((row, Subjects::One(id)))) if row.subject() == Subject::Joint => Some((
            GraphKey::Joint(id),
            joint_sensors(id).iter().position(|(_, s)| s == source)?,
        )),
        (_, Some((_, Subjects::One(id)))) => Some((
            GraphKey::Body(id),
            body_sensors(id).iter().position(|(_, s)| s == source)?,
        )),
        _ => None,
    }
}

/// The `(GraphKey, input index)` a binding sink resolves to — body actuator
/// pins in [`body_actuators`] order, a joint's motor pins in
/// [`joint_actuators`] order, a thruster's thrust pin, or the Scope block's
/// input for `Plot`.
fn sink_pin(sink: &SignalSink) -> (GraphKey, usize) {
    let at = |pins: &[(&str, SignalSink)]| pins.iter().position(|(_, s)| s == sink).unwrap_or(0);
    match sink {
        SignalSink::Fill(id)
        | SignalSink::TracerColor(id)
        | SignalSink::FieldStrength(id)
        | SignalSink::GravityScale(id) => (GraphKey::Body(*id), at(&body_actuators(*id))),
        SignalSink::MotorSpeed(id) | SignalSink::MotorCap(id) => {
            (GraphKey::Joint(*id), at(&joint_actuators(*id)))
        }
        SignalSink::Thrust(id) => (GraphKey::Node(*id), 0),
        SignalSink::Plot => (GraphKey::Scope, 0),
    }
}

/// The node-graph pane's full ECS read/write set, bundled as one
/// [`SystemParam`] so the bottom-dock host stays under Bevy's system-parameter
/// cap: the config-seam resources (bindings / params / computed / the graph
/// view-state), the scene reads the live per-pin readouts need, and the
/// selection the highlight + "locate" use. The bus is shared with the plot pane,
/// so the host owns it separately.
#[derive(SystemParam)]
pub struct GraphParams<'w, 's> {
    pub(crate) graph: ResMut<'w, NodeGraph>,
    pub(crate) bindings: ResMut<'w, SignalBindings>,
    /// The authored signal graph (params, blocks, their positions).
    pub(crate) signal_graph: ResMut<'w, Graph>,
    pub(crate) bodies: Query<'w, 's, (&'static StableId, &'static ShapeDef), With<Body>>,
    pub(crate) joints: Query<'w, 's, (&'static StableId, &'static JointDef), With<Joint>>,
    /// The behavior nodes, for thruster blocks (brief N-3).
    pub(crate) nodes: Query<
        'w,
        's,
        (
            &'static StableId,
            &'static NodeKind,
            &'static NodeAttachment,
        ),
        With<BehaviorNode>,
    >,
    pub(crate) selection: ResMut<'w, Selection>,
    pub(crate) ids: Query<'w, 's, &'static StableId>,
    pub(crate) physics: PhysicsQueries<'w, 's>,
    pub(crate) body_transforms: Query<'w, 's, &'static Transform, With<Body>>,
    pub(crate) index: Res<'w, IdIndex>,
    pub(crate) fixed: Res<'w, Time<Fixed>>,
    /// The editor's bare-key uses, for a key input's clash warning.
    pub(crate) editor_keys: Res<'w, EditorKeyTable>,
}

/// Reconciles the canvas blocks + wires from the live dataflow and builds the
/// frame's [`GraphViewer`] (block highlights + the live per-pin readouts). The
/// bottom-dock host calls this before rendering; [`node_graph_section`] draws
/// the returned viewer and [`apply_pane`] drains its edits afterward.
pub(crate) fn prepare(gp: &mut GraphParams, bus: &SignalBus) -> GraphViewer {
    let desired = collect_desired(
        &gp.graph.added,
        &gp.graph.names,
        &gp.graph.comments,
        &gp.bindings,
        &gp.signal_graph,
        &gp.bodies,
        &gp.joints,
        &gp.nodes,
    );
    reconcile(&mut gp.graph, desired);
    rebuild_wires(&mut gp.graph, &gp.bindings, &gp.signal_graph);

    // Precompute the live value of every output/input pin (the Simulink
    // "watch the signal flow" readout).
    let dt = gp.fixed.timestep().as_secs_f32().max(1e-6);
    let output_values = output_pin_values(
        &gp.graph.snarl,
        &gp.index,
        &gp.physics,
        &gp.body_transforms,
        bus,
        dt,
    );
    let input_values = binding_input_values(&gp.bindings, bus);

    // Scene selection → highlight the matching body blocks.
    let selected: HashSet<StableId> = gp
        .selection
        .iter()
        .filter_map(|e| gp.ids.get(e).ok().copied())
        .collect();
    GraphViewer {
        selected,
        output_values,
        input_values,
        bindings: gp.bindings.0.clone(),
        labels: gp
            .signal_graph
            .nodes
            .iter()
            .filter_map(|n| n.label.clone())
            .collect(),
        editor_keys: gp.editor_keys.clone(),
        ..GraphViewer::default()
    }
}

/// The canvas toolbar.
///
/// The header used to be a title, a fit button, and one grey sentence of hint
/// text that was the only place three separate gestures were documented — and
/// it stayed on screen forever, spending the width that the canvas wanted.
/// The gestures are now tooltips on the controls they belong to, and the block
/// palette (previously reachable only by right-clicking empty canvas) is a
/// button, so adding a block does not require knowing it is there.
fn toolbar(ui: &mut egui::Ui, graph: &mut NodeGraph, viewer: &mut GraphViewer) {
    ui.horizontal(|ui| {
        widgets::section_header(ui, "Node Graph");
        ui.menu_button("+ Add", |ui| {
            viewer.add_menu(ui);
        })
        .response
        .on_hover_text("add a block — also on right-click over empty canvas");
        if ui
            .small_button(crate::fonts::glyph::FIT)
            .on_hover_text("zoom to fit the whole graph")
            .clicked()
        {
            graph.fit_requested = true;
        }
        ui.separator();
        // The gestures that have no button. Kept as one hover target rather
        // than a permanent sentence, so the width goes to the canvas.
        ui.label(egui::RichText::new(crate::fonts::glyph::PARAM).weak())
            .on_hover_text(
                "Shift-drag empty canvas: box-select · drag a selected block: move them all\n\
                 Drag a sensor pin onto an actuator pin: bind them\n\
                 Right-click a body in the scene ▸ Add to node editor",
            );
    });
}

/// Renders the node-graph canvas into `ui` (its dock pane): the toolbar, the canvas-following dot-grid, and the snarl node view. Persists this
/// frame's pan/zoom transform and retires a fulfilled fit request. The user's
/// connect/disconnect gestures land in `viewer`, drained by [`apply_pane`].
pub(crate) fn node_graph_section(
    ui: &mut egui::Ui,
    graph: &mut NodeGraph,
    viewer: &mut GraphViewer,
) {
    toolbar(ui, graph, viewer);
    if graph.keys.is_empty() {
        widgets::empty_state(
            ui,
            "Empty — add a body from its right-click menu, or add params/computed.",
        );
        return;
    }
    // The canvas-following dot-grid (last frame's transform), behind the
    // blocks; then hand the view rect + fit request to the viewer.
    if let Some(t) = graph.last_transform {
        paint_dot_grid(ui.painter(), ui.max_rect(), t);
    }
    viewer.fit = graph.fit_requested;
    viewer.view_rect = Some(ui.max_rect());
    let tokens = crate::theme::tokens(ui.ctx());
    viewer.selected_stroke = tokens.accent;
    let style = themed_style(&tokens);
    graph.snarl.show(viewer, &style, "node-graph", ui);
    // A settled drag (no button held) writes each moved block's position back
    // to the graph: one `settings` step, like a slider drag.
    if !ui.ctx().input(|i| i.pointer.any_down()) {
        for (_, pos, data) in graph.snarl.nodes_pos_ids() {
            if let (Some(node), Some(saved)) = (data.signal_node(), data.saved_pos())
                && pos != saved
            {
                viewer.graph_edits.push(GraphEdit::SetPos {
                    node: NodeRef::Id(node.id),
                    pos: Vec2::new(pos.x, pos.y),
                });
            }
        }
    }
    // Persist the pan/zoom transform (for next frame's grid) and retire a
    // fulfilled fit request.
    if let Some(t) = viewer.captured_transform {
        graph.last_transform = Some(t);
    }
    if viewer.fit_done {
        graph.fit_requested = false;
    }
}

/// Drains the frame's [`GraphViewer`] into the scene: a "locate" click selects
/// the body, then every authoring/binding edit flows to the config-seam
/// resources via [`apply_viewer_edits`]. Called by the dock host after the
/// pane renders.
pub(crate) fn apply_pane(viewer: GraphViewer, gp: &mut GraphParams) {
    // A "locate" click selects the body in the scene (the inspector follows).
    if let Some(id) = viewer.select_body
        && let Some(entity) = gp.index.entity(id)
    {
        gp.selection.set(entity);
    }
    apply_viewer_edits(viewer, &mut gp.graph, &mut gp.bindings, &mut gp.signal_graph);
}

/// Applies a frame's graph edits in order, each validated
/// ([`Graph::edit`]); a refused edit is logged and skipped. The resource is
/// written only when the graph changed, so an idle canvas never marks it
/// changed (which would recompile the signals every frame).
pub(crate) fn apply_graph_edits(edits: Vec<GraphEdit>, graph: &mut ResMut<Graph>) {
    if edits.is_empty() {
        return;
    }
    let mut next = (**graph).clone();
    for edit in &edits {
        if let Err(error) = next.edit(edit) {
            warn!(%error, "canvas graph edit refused");
        }
    }
    if next != **graph {
        **graph = next;
    }
}

/// Drains a frame's [`GraphViewer`] edits into the config-seam resources:
/// binding create/remove/transfer, custom names and comments (view state),
/// and every signal-graph edit (add, wire, constants, positions, delete).
fn apply_viewer_edits(
    viewer: GraphViewer,
    graph: &mut NodeGraph,
    bindings: &mut SignalBindings,
    signal_graph: &mut ResMut<Graph>,
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
    // Comment lifecycle: add, edit, delete — all view state on `NodeGraph`.
    if viewer.add_comment {
        let id = graph.next_comment;
        graph.next_comment += 1;
        graph.comments.push((id, String::new()));
    }
    for (id, text) in viewer.comment_edits {
        if let Some(slot) = graph.comments.iter_mut().find(|(c, _)| *c == id) {
            slot.1 = text;
        }
    }
    if !viewer.delete_comments.is_empty() {
        graph
            .comments
            .retain(|(id, _)| !viewer.delete_comments.contains(id));
    }
    for (id, name) in viewer.renames {
        if name.trim().is_empty() {
            graph.names.remove(&id);
        } else {
            graph.names.insert(id, name);
        }
    }
    for id in viewer.remove_bodies {
        graph.added.remove(&id);
    }
    apply_graph_edits(viewer.graph_edits, signal_graph);
}

/// A fresh unique `<base>-N` label over `taken` (`param-1`, `gain-2`).
fn fresh_label(taken: &[String], base: &str) -> String {
    (1..=taken.len() + 1)
        .map(|n| format!("{base}-{n}"))
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| base.to_owned())
}

/// The node-canvas theme (vvvv-flavoured): sharp corners, flat fills, thin
/// strokes, no gradients — the faint dot-grid is painted separately, so the
/// snarl background pattern is off.
fn themed_style(tokens: &crate::theme::ChromeTheme) -> SnarlStyle {
    let node_frame = egui::Frame::new()
        .fill(tokens.node)
        .stroke(egui::Stroke::new(1.0, tokens.node_outline))
        .corner_radius(egui::CornerRadius::same(0))
        .inner_margin(egui::Margin::same(6));
    let header_frame = egui::Frame::new()
        .fill(tokens.node_header)
        .corner_radius(egui::CornerRadius::same(0))
        .inner_margin(egui::Margin::symmetric(6, 3));
    SnarlStyle {
        // Simulink-style right-angle wires; sharp corners (vvvv).
        wire_style: Some(WireStyle::AxisAligned { corner_radius: 0.0 }),
        wire_width: Some(1.5),
        node_frame: Some(node_frame),
        header_frame: Some(header_frame),
        bg_frame: Some(egui::Frame::new().fill(tokens.canvas)),
        bg_pattern: Some(BackgroundPattern::NoPattern),
        pin_size: Some(4.0),
        ..SnarlStyle::new()
    }
}

/// The live value of every block's output pins (sensors through the shared
/// reader, params/computed off the bus) — the "watch the signal flow" readout.
fn output_pin_values(
    snarl: &Snarl<NodeData>,
    index: &IdIndex,
    physics: &PhysicsQueries,
    body_transforms: &Query<&Transform, With<Body>>,
    bus: &SignalBus,
    dt: f32,
) -> HashMap<GraphKey, Vec<Option<f32>>> {
    let mut values: HashMap<GraphKey, Vec<Option<f32>>> = HashMap::new();
    for (_, data) in snarl.node_ids() {
        let vals: Vec<Option<f32>> = match data {
            NodeData::Body { id, .. } => body_sensors(*id)
                .iter()
                .map(|(_, s)| read_source(s, index, physics, body_transforms, dt))
                .collect(),
            NodeData::Joint { id, .. } => joint_sensors(*id)
                .iter()
                .map(|(_, s)| read_source(s, index, physics, body_transforms, dt))
                .collect(),
            NodeData::Param(node) | NodeData::Computed(node) => {
                vec![node.label.as_deref().and_then(|label| bus.get(label))]
            }
            NodeData::Thruster { .. } | NodeData::Scope | NodeData::Comment { .. } => Vec::new(),
        };
        values.insert(data.key(), vals);
    }
    values
}

/// Snarl's zoom range (its defaults) — the fit transform clamps to it.
const MIN_SCALE: f32 = 0.2;
const MAX_SCALE: f32 = 2.0;

/// The pan/zoom transform that frames every node position into `view` — snarl's
/// own initial-fit (expand the bounding box, center it, clamp the scale). Pure,
/// so zoom-to-fit is unit-testable without a window. `None` when there are no
/// nodes to frame.
fn fit_transform(
    node_positions: &[egui::Pos2],
    view: egui::Rect,
) -> Option<egui::emath::TSTransform> {
    let mut bb = egui::Rect::NOTHING;
    for &p in node_positions {
        bb.extend_with(p);
    }
    if !bb.is_finite() {
        return None;
    }
    let bb = bb.expand(80.0); // node bodies extend past their top-left anchor
    let scaling = (view.width() / bb.width())
        .min(view.height() / bb.height())
        .clamp(MIN_SCALE, MAX_SCALE);
    let translation = view.center().to_vec2() - scaling * bb.center().to_vec2();
    Some(egui::emath::TSTransform {
        scaling,
        translation,
    })
}

/// Paints a faint dot-grid over `rect` aligned to `t`, so the dots pan and zoom
/// with the canvas (the vvvv graph-paper feel). A power-of-two level-of-detail
/// keeps the on-screen dot spacing — and the dot count — bounded at any zoom.
fn paint_dot_grid(painter: &egui::Painter, rect: egui::Rect, t: egui::emath::TSTransform) {
    let mut spacing = 40.0_f32; // graph-space px between dots
    while spacing * t.scaling < 16.0 {
        spacing *= 2.0; // LOD: never denser than ~16px on screen
    }
    let inv = t.inverse();
    let top_left = inv.mul_pos(rect.left_top());
    let bottom_right = inv.mul_pos(rect.right_bottom());
    let dot = crate::theme::tokens(painter.ctx()).canvas_dot;
    let mut gx = (top_left.x / spacing).floor() * spacing;
    while gx <= bottom_right.x {
        let mut gy = (top_left.y / spacing).floor() * spacing;
        while gy <= bottom_right.y {
            painter.circle_filled(t.mul_pos(egui::pos2(gx, gy)), 1.0, dot);
            gy += spacing;
        }
        gx += spacing;
    }
}

/// Builds the desired block set: a node per **explicitly added** body or body
/// **referenced by a binding** (never merely selected — that would clutter the
/// canvas), plus every param and computed signal.
fn collect_desired(
    added: &HashSet<StableId>,
    names: &HashMap<StableId, String>,
    comments: &[(u32, String)],
    bindings: &SignalBindings,
    signal_graph: &Graph,
    bodies: &Query<(&StableId, &ShapeDef), With<Body>>,
    joints: &Query<(&StableId, &JointDef), With<Joint>>,
    nodes: &Query<(&StableId, &NodeKind, &NodeAttachment), With<BehaviorNode>>,
) -> Vec<NodeData> {
    let kinds: HashMap<StableId, &'static str> = bodies
        .iter()
        .map(|(id, shape)| (*id, shape.kind_label()))
        .collect();
    let live: HashSet<StableId> = kinds.keys().copied().collect();
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
    // Joints a binding names (a joint sensor source or a motor sink).
    let mut shown_joints: HashSet<StableId> = HashSet::new();
    for binding in &bindings.0 {
        if let Some((GraphKey::Joint(id), _)) = source_pin(&binding.source, signal_graph) {
            shown_joints.insert(id);
        }
        if let (GraphKey::Joint(id), _) = sink_pin(&binding.sink) {
            shown_joints.insert(id);
        }
    }
    // Bodies and joints feeding a signal node (a canonical sensor name it
    // reads from the bus).
    for node in &signal_graph.nodes {
        for input in &node.inputs {
            let Input::External(name) = input else {
                continue;
            };
            match SignalSource::from_bus_name(name).and_then(|s| source_pin(&s, signal_graph)) {
                Some((GraphKey::Body(id), _)) if live.contains(&id) => {
                    shown.insert(id);
                }
                Some((GraphKey::Joint(id), _)) => {
                    shown_joints.insert(id);
                }
                _ => {}
            }
        }
    }

    // Every signal node of the graph, params first. (Shape nodes join the
    // canvas with brief G-4.)
    let mut items: Vec<NodeData> = Vec::new();
    let signal_nodes = || {
        signal_graph
            .nodes
            .iter()
            .filter(|n| n.kind.family() == Family::Signal)
    };
    items.extend(
        signal_nodes()
            .filter(|n| matches!(n.kind, SignalKind::Param { .. }))
            .map(|n| NodeData::Param(n.clone())),
    );
    items.extend(
        signal_nodes()
            .filter(|n| !matches!(n.kind, SignalKind::Param { .. }))
            .map(|n| NodeData::Computed(n.clone())),
    );
    // A joint block for every joint a wire names, and for every motorised
    // hinge or slider on a body already on the canvas: its motor pins are
    // where a drive is wired (brief N-1).
    for (id, def) in joints {
        let motor = joint_motor(def);
        let on_canvas = def.referenced_bodies().any(|b| shown.contains(&b));
        if shown_joints.contains(id) || (motor.is_some() && on_canvas) {
            items.push(NodeData::Joint {
                id: *id,
                kind: joint_kind_label(&def.kind),
                motor: motor.is_some(),
                linear: motor.unwrap_or(false),
            });
        }
    }
    // A thruster block for every thruster a wire names, and for every
    // thruster on a body already on the canvas: its thrust pin is where a
    // drive is wired (brief N-3).
    for (id, kind, attachment) in nodes {
        if !matches!(kind, NodeKind::Thruster(_)) {
            continue;
        }
        let named = bindings
            .0
            .iter()
            .any(|b| sink_pin(&b.sink).0 == GraphKey::Node(*id));
        let on_canvas = attachment.target.is_some_and(|b| shown.contains(&b));
        if named || on_canvas {
            items.push(NodeData::Thruster { id: *id });
        }
    }
    for id in shown {
        items.push(NodeData::Body {
            id,
            name: names.get(&id).cloned(),
            kind: kinds.get(&id).copied().unwrap_or("body"),
        });
    }
    // The Scope sink is a fixture whenever there's anything to plot into it.
    if !items.is_empty() {
        items.push(NodeData::Scope);
        // Comments last, so a new note lands after the graph in reconcile order.
        items.extend(comments.iter().map(|(id, text)| NodeData::Comment {
            id: *id,
            text: text.clone(),
        }));
    }
    items
}

/// Whether a joint has a motor a drive can reach: `Some(linear)` for a
/// motorised hinge (`false`) or slider (`true`), `None` otherwise.
fn joint_motor(def: &JointDef) -> Option<bool> {
    match &def.kind {
        JointKind::Hinge { motor: Some(_), .. } => Some(false),
        JointKind::Slider { motor: Some(_), .. } => Some(true),
        JointKind::Hinge { .. } | JointKind::Slider { .. } | JointKind::Spring { .. } => None,
    }
}

/// A joint block's title: its kind.
const fn joint_kind_label(kind: &JointKind) -> &'static str {
    match kind {
        JointKind::Hinge { .. } => "hinge",
        JointKind::Slider { .. } => "slider",
        JointKind::Spring { .. } => "spring",
    }
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
    // A joint sensor references a joint, not a body: no body block.
    if let Some((row, subjects)) = binding.source.reading()
        && row.subject() != Subject::Joint
    {
        match subjects {
            Subjects::One(id) => ids.push(id),
            Subjects::Two(a, b) => ids.extend([a, b]),
        }
    }
    match &binding.sink {
        SignalSink::Fill(id)
        | SignalSink::TracerColor(id)
        | SignalSink::FieldStrength(id)
        | SignalSink::GravityScale(id) => ids.push(*id),
        SignalSink::Plot
        | SignalSink::MotorSpeed(_)
        | SignalSink::MotorCap(_)
        | SignalSink::Thrust(_) => {}
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
    comment: f32,
}

impl ColumnCursor {
    fn next(&mut self, role: Role) -> egui::Pos2 {
        let slot = match role {
            Role::Producer => &mut self.producer,
            Role::Modulator => &mut self.modulator,
            Role::Body => &mut self.body,
            Role::Scope => &mut self.scope,
            Role::Comment => &mut self.comment,
        };
        let y = *slot + 16.0;
        *slot = y + 96.0;
        egui::pos2(role.column_x(), y)
    }
}

/// Rebuilds the snarl graph to mirror `desired`, preserving dragged positions.
/// A signal block sits at its saved position: placed there when it appears,
/// and moved there when the saved position changes (undo, `graph-set`); a
/// view-only block (body, joint, scope, note) stacks down its column.
fn reconcile(graph: &mut NodeGraph, desired: Vec<NodeData>) {
    let mut cursor = ColumnCursor::default();
    let mut seen: HashSet<GraphKey> = HashSet::with_capacity(desired.len());
    for node in desired {
        let key = node.key();
        seen.insert(key.clone());
        let saved = node.saved_pos();
        if let Some(&id) = graph.keys.get(&key)
            && let Some(slot) = graph.snarl.get_node_info_mut(id)
        {
            slot.value = node;
            if let Some(saved) = saved
                && graph.placed.get(&key) != Some(&saved)
            {
                slot.pos = saved;
                graph.placed.insert(key, saved);
            }
            continue;
        }
        let pos = saved.unwrap_or_else(|| cursor.next(node.role()));
        if let Some(saved) = saved {
            graph.placed.insert(key.clone(), saved);
        }
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
        graph.placed.remove(&key);
        graph.keys.remove(&key);
    }
}

/// The snarl viewer: labels each block's pins and turns user connect/disconnect
/// gestures into binding edits (collected, applied by the system). Wires shown
/// are the current bindings; it never owns the graph.
#[derive(Default)]
pub(crate) struct GraphViewer {
    /// Scene-selected body ids — their blocks get a highlight frame.
    selected: HashSet<StableId>,
    /// The highlight frame's colour: the scene accent (chrome token).
    selected_stroke: egui::Color32,
    /// Live value of each output pin, `[key][output]` (the running readout).
    output_values: HashMap<GraphKey, Vec<Option<f32>>>,
    /// Live value driving each bound input pin, `[key][input]`.
    input_values: HashMap<GraphKey, Vec<Option<f32>>>,
    /// A snapshot of the bindings, so a body footer can render + edit the
    /// transfer (domain + gradient) of the wires driving it.
    bindings: Vec<SignalBinding>,
    /// Every label in the signal graph, for fresh block names.
    labels: Vec<String>,
    /// The frame's signal-graph edits, applied in order after the pane draws.
    graph_edits: Vec<GraphEdit>,
    /// The editor's bare-key uses, so a key input warns of a clash.
    editor_keys: EditorKeyTable,
    new_bindings: Vec<SignalBinding>,
    removed: Vec<(SignalSource, SignalSink)>,
    /// Transfer edits `(source, sink, map, gradient)` from a body footer.
    transfer_edits: Vec<(SignalSource, SignalSink, SignalMap, GradientSpec)>,
    /// Custom-name edits `(body, new name)` from the block's rename field.
    renames: Vec<(StableId, String)>,
    /// Comment text edits, keyed by serial → [`NodeGraph::comments`].
    comment_edits: Vec<(u32, String)>,
    /// Comments the user deleted.
    delete_comments: Vec<u32>,
    /// Set when the palette's "note" entry was chosen.
    add_comment: bool,
    /// A body block whose "locate" button was clicked — select it in the
    /// scene (block → object → inspector, the reverse of the highlight).
    select_body: Option<StableId>,
    /// Whether a zoom-to-fit was requested this frame (from the header button).
    fit: bool,
    /// The snarl canvas rect, set before `show` so `current_transform` can fit
    /// the graph into it.
    view_rect: Option<egui::Rect>,
    /// The pan/zoom transform snarl used this frame (for the dot-grid).
    captured_transform: Option<egui::emath::TSTransform>,
    /// Whether the fit actually ran (so the request flag can be cleared).
    fit_done: bool,
    /// Bodies taken off the canvas (view state).
    remove_bodies: Vec<StableId>,
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

    /// A fresh `wire-N` binding name, unique over the existing bindings *and*
    /// the ones already made this frame. A binding's name is its **bus name**
    /// (the plot-legend key), so a collision would clobber another wire's
    /// published value — the count-based name did exactly that when a `wire-N`
    /// already existed.
    fn fresh_wire_name(&self) -> String {
        let taken = |name: &str| {
            self.bindings
                .iter()
                .chain(self.new_bindings.iter())
                .any(|b| b.name == name)
        };
        (1..=self.bindings.len() + self.new_bindings.len() + 1)
            .map(|n| format!("wire-{n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| "wire".to_owned())
    }

    /// What an output pin feeds into a signal node's port: a wire from a
    /// signal node (by id, so renaming it keeps the wire), or a bus read of
    /// a body or joint sensor's canonical name.
    fn out_input(snarl: &Snarl<NodeData>, pin: OutPinId) -> Option<Input> {
        let data = snarl.get_node(pin.node)?;
        if let Some(node) = data.signal_node() {
            return Some(Input::wire(node.id));
        }
        data.output_source(pin.output)?
            .bus_name()
            .map(Input::External)
    }

    /// If `pin` is a signal block's operand pin, its node and the graph port
    /// the pin stands for.
    fn port_at(snarl: &Snarl<NodeData>, pin: InPinId) -> Option<(&GraphNode, usize)> {
        match snarl.get_node(pin.node)? {
            NodeData::Computed(node) => operand_ports(&node.kind)
                .get(pin.input)
                .map(|&port| (node, port)),
            _ => None,
        }
    }

    /// The edit that unbinds port `port` of `node`: its default constant.
    fn unbind(node: &GraphNode, port: usize) -> Option<GraphEdit> {
        let default = node.kind.input_ports().get(port)?.default.value();
        Some(GraphEdit::SetInput {
            node: NodeRef::Id(node.id),
            port: PortRef::Index(port),
            input: Input::Const(default),
        })
    }

    /// Adds a block of `kind` with `inputs`, labelled `<kind>-N`.
    fn add_block(&mut self, kind: SignalKind, inputs: Vec<Input>) {
        let base = gradiance_graph::meta(&kind).name.to_lowercase();
        let label = fresh_label(&self.labels, &base);
        self.labels.push(label.clone());
        self.graph_edits.push(GraphEdit::Add {
            id: StableId::new(),
            kind,
            label: Some(label),
            inputs,
        });
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
fn pin_label(name: &str, value: Option<f32>, unit: &str) -> String {
    match value {
        Some(v) if unit.is_empty() => format!("{name}  {v:.2}"),
        Some(v) => format!("{name}  {v:.2} {unit}"),
        None => name.to_owned(),
    }
}

/// The block palette, shared by the toolbar button and the canvas right-click
/// menu — one definition, so a new block appears in both.
impl GraphViewer {
    pub(crate) fn add_menu(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "Add block");
        if ui
            .button("Note")
            .on_hover_text("a free-floating annotation — no pins, no effect on the graph")
            .clicked()
        {
            self.add_comment = true;
            ui.close();
        }
        ui.menu_button("Input", |ui| {
            if ui.button("⊙ Parameter (slider)").clicked() {
                let name = fresh_label(&self.labels, "param");
                self.labels.push(name.clone());
                self.graph_edits.push(GraphEdit::Param {
                    id: StableId::new(),
                    name,
                    value: 0.5,
                    min: 0.0,
                    max: 1.0,
                });
                ui.close();
            }
            if ui.button("# Constant").clicked() {
                self.add_block(SignalKind::Constant, vec![Input::scalar(1.0)]);
                ui.close();
            }
            if ui.button("ƒ Time").clicked() {
                self.add_block(SignalKind::Time, Vec::new());
                ui.close();
            }
            if ui.button("ƒ Oscillator").clicked() {
                self.add_block(SignalKind::Oscillator, Vec::new());
                ui.close();
            }
            if ui
                .button("Key")
                .on_hover_text("a held key in play: E gives +1, Q gives -1 (set them in the block)")
                .clicked()
            {
                self.add_block(
                    SignalKind::Key {
                        key: "E".to_owned(),
                        negative: Some("Q".to_owned()),
                    },
                    Vec::new(),
                );
                ui.close();
            }
        });
        ui.menu_button("Modulation", |ui| {
            let mut add = |ui: &mut egui::Ui, label: &str, kind: SignalKind| {
                if ui.button(label).clicked() {
                    self.add_block(kind, Vec::new());
                    ui.close();
                }
            };
            add(ui, "ƒ Gain (× k)", SignalKind::Gain);
            add(ui, "ƒ Sum (a + b)", SignalKind::Sum);
            add(ui, "ƒ Sub (a − b)", SignalKind::Sub);
            add(ui, "ƒ Product (a · b)", SignalKind::Product);
            add(ui, "ƒ Min (a, b)", SignalKind::Min);
            add(ui, "ƒ Max (a, b)", SignalKind::Max);
            add(ui, "ƒ Abs |in|", SignalKind::Abs);
            add(ui, "ƒ Curve (shape in)", SignalKind::Curve);
        });
        widgets::empty_state(ui, "Output: the ▭ Scope block is always present.");
    }
}

impl SnarlViewer<NodeData> for GraphViewer {
    fn title(&mut self, node: &NodeData) -> String {
        node.title()
    }

    fn current_transform(
        &mut self,
        to_global: &mut egui::emath::TSTransform,
        snarl: &mut Snarl<NodeData>,
    ) {
        // Fulfil a pending zoom-to-fit before capturing the transform.
        if self.fit
            && let Some(view) = self.view_rect
        {
            let positions: Vec<egui::Pos2> = snarl.nodes_pos().map(|(pos, _)| pos).collect();
            if let Some(t) = fit_transform(&positions, view) {
                *to_global = t;
                self.fit_done = true;
            }
        }
        // Remember it so next frame's dot-grid aligns to the canvas.
        self.captured_transform = Some(*to_global);
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
            return default.stroke(egui::Stroke::new(2.0, self.selected_stroke));
        }
        default
    }

    fn has_body(&mut self, node: &NodeData) -> bool {
        if matches!(node, NodeData::Comment { .. }) {
            return true;
        }
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
        // A comment is the one block whose body *is* its content.
        if let Some(NodeData::Comment { id, text }) = snarl.get_node(node) {
            let (id, mut text) = (*id, text.clone());
            if ui
                .add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_width(140.0)
                        .desired_rows(2)
                        .hint_text("note"),
                )
                .changed()
            {
                self.comment_edits.push((id, text));
            }
            return;
        }
        let Some(NodeData::Body { id, name, .. }) = snarl.get_node(node) else {
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
            NodeData::Joint { id, motor, .. } => {
                if *motor {
                    joint_actuators(*id).len()
                } else {
                    0
                }
            }
            NodeData::Thruster { id } => thruster_actuators(*id).len(),
            NodeData::Param(_) | NodeData::Comment { .. } => 0,
            NodeData::Computed(node) => operand_ports(&node.kind).len(),
            NodeData::Scope => 1,
        }
    }

    fn outputs(&mut self, node: &NodeData) -> usize {
        match node {
            NodeData::Body { id, .. } => body_sensors(*id).len(),
            NodeData::Joint { id, .. } => joint_sensors(*id).len(),
            NodeData::Param(_) | NodeData::Computed(_) => 1,
            NodeData::Thruster { .. } | NodeData::Scope | NodeData::Comment { .. } => 0,
        }
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        // An actuator pin carries its unit: the colour channels take a
        // dimensionless transfer, the physical ones (brief N-1) an SI value.
        let actuator = |pins: &[(&'static str, SignalSink)], linear: bool| {
            pins.get(pin.id.input).map_or_else(
                || (String::new(), ""),
                |(l, s)| ((*l).to_owned(), s.dimension(linear).symbol()),
            )
        };
        let (name, unit) = match snarl.get_node(pin.id.node) {
            Some(NodeData::Body { id, .. }) => actuator(&body_actuators(*id), false),
            Some(NodeData::Joint { id, linear, .. }) => actuator(&joint_actuators(*id), *linear),
            Some(NodeData::Thruster { id }) => actuator(&thruster_actuators(*id), false),
            Some(NodeData::Computed(node)) => (
                operand_ports(&node.kind)
                    .get(pin.id.input)
                    .and_then(|&port| node.kind.input_name(port))
                    .map_or_else(String::new, |name| name.into_owned()),
                "",
            ),
            Some(NodeData::Scope) => ("signals".to_owned(), ""),
            _ => (String::new(), ""),
        };
        let key = snarl.get_node(pin.id.node).map(NodeData::key);
        let value = key
            .as_ref()
            .and_then(|k| Self::value_at(&self.input_values, k, pin.id.input));
        ui.label(pin_label(&name, value, unit));
        PinInfo::circle().with_fill(crate::theme::tokens(ui.ctx()).pin_in)
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        // Sensor output ports carry the SI unit of their quantity (P3 catalog).
        let (name, unit) = match snarl.get_node(pin.id.node) {
            Some(NodeData::Body { id, .. }) => body_sensors(*id).get(pin.id.output).map_or_else(
                || (String::new(), ""),
                |(l, s)| ((*l).to_owned(), s.dimension().symbol()),
            ),
            Some(NodeData::Joint { id, .. }) => joint_sensors(*id).get(pin.id.output).map_or_else(
                || (String::new(), ""),
                |(l, s)| ((*l).to_owned(), s.dimension().symbol()),
            ),
            Some(NodeData::Param(node) | NodeData::Computed(node)) => (
                node.label.clone().unwrap_or_else(|| "out".to_owned()),
                "",
            ),
            Some(NodeData::Thruster { .. } | NodeData::Scope | NodeData::Comment { .. }) | None => {
                (String::new(), "")
            }
        };
        let key = snarl.get_node(pin.id.node).map(NodeData::key);
        let value = key
            .as_ref()
            .and_then(|k| Self::value_at(&self.output_values, k, pin.id.output));
        ui.label(pin_label(&name, value, unit));
        PinInfo::circle().with_fill(crate::theme::tokens(ui.ctx()).pin_out)
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeData>) {
        // A wire into a signal block's operand: a graph wire by id from a
        // signal node, or a bus read of a sensor's canonical name.
        if let Some((node, port)) = Self::port_at(snarl, to.id) {
            if let Some(input) = Self::out_input(snarl, from.id) {
                self.graph_edits.push(GraphEdit::SetInput {
                    node: NodeRef::Id(node.id),
                    port: PortRef::Index(port),
                    input,
                });
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
                    name: self.fresh_wire_name(),
                    source,
                    map: SignalMap::default(),
                    curve: None,
                    gradient: GradientSpec::default(),
                    sink,
                });
            }
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeData>) {
        if let Some((node, port)) = Self::port_at(snarl, to.id) {
            self.graph_edits.extend(Self::unbind(node, port));
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
        if let Some((node, port)) = Self::port_at(snarl, pin.id) {
            self.graph_edits.extend(Self::unbind(node, port));
            return;
        }
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
        self.add_menu(ui);
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
            Some(NodeData::Param(node)) => {
                let id = node.id;
                if ui.button("Delete parameter").clicked() {
                    self.graph_edits.push(GraphEdit::Remove {
                        node: NodeRef::Id(id),
                    });
                    ui.close();
                }
            }
            Some(NodeData::Computed(node)) => {
                let id = node.id;
                if ui.button("Delete computed").clicked() {
                    self.graph_edits.push(GraphEdit::Remove {
                        node: NodeRef::Id(id),
                    });
                    ui.close();
                }
            }
            Some(NodeData::Comment { id, .. }) => {
                let id = *id;
                if ui.button("Delete note").clicked() {
                    self.delete_comments.push(id);
                    ui.close();
                }
            }
            Some(NodeData::Joint { .. } | NodeData::Thruster { .. }) => {
                widgets::empty_state(ui, "shown while a wire or a body on the canvas names it");
            }
            Some(NodeData::Scope) | None => {
                widgets::empty_state(ui, "the plot output");
            }
        }
    }

    // A footer configures a block in place: a body edits the transfer of each
    // wire driving it; a modulation block edits its constants (k / amp / freq).
    fn has_footer(&mut self, node: &NodeData) -> bool {
        match node {
            NodeData::Body { .. } | NodeData::Param(_) => true,
            NodeData::Computed(node) => matches!(
                node.kind,
                SignalKind::Gain
                    | SignalKind::Oscillator
                    | SignalKind::Constant
                    | SignalKind::Curve
                    | SignalKind::Key { .. }
            ),
            _ => false,
        }
    }

    fn show_footer(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<NodeData>,
    ) {
        // A block's constants (its non-pin ports, a key's keys).
        if let Some(NodeData::Computed(block)) = snarl.get_node(node) {
            self.graph_edits
                .extend(block_constants(ui, block, &self.editor_keys));
            return;
        }
        // A param block's value — the same tunable knob as the Signals dock,
        // edited in place (Simulink source-block-style).
        if let Some(NodeData::Param(param)) = snarl.get_node(node) {
            if let SignalKind::Param { value, min, max } = param.kind {
                let mut next = value;
                if ui.add(egui::Slider::new(&mut next, min..=max)).changed() {
                    self.graph_edits.push(GraphEdit::SetKind {
                        node: NodeRef::Id(param.id),
                        kind: SignalKind::Param {
                            value: next,
                            min,
                            max,
                        },
                    });
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
            // An actuator takes the raw value: no domain or gradient to edit.
            if key != GraphKey::Body(id) || binding.sink.actuator().is_some() {
                continue;
            }
            let mut map = binding.map;
            let mut gradient = binding.gradient;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.small(binding.sink.label());
                let rest = SignalMap::default();
                changed |= widgets::config_field(rest.in_min, 1.0)
                    .prefix("[")
                    .show(ui, &mut map.in_min)
                    .changed();
                changed |= widgets::config_field(rest.in_max, 1.0)
                    .suffix("]")
                    .show(ui, &mut map.in_max)
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

/// A block's footer: its constant ports (a gain's `k`, an oscillator's `amp`
/// and `freq`, a constant's `value`, a curve) and a key's keys. Returns the
/// edits a change makes. A port a script wired shows as wired, not a field.
fn block_constants(
    ui: &mut egui::Ui,
    node: &GraphNode,
    editor_keys: &EditorKeyTable,
) -> Vec<GraphEdit> {
    let mut edits = Vec::new();
    let set = |port: usize, value: gradiance_graph::Value| GraphEdit::SetInput {
        node: NodeRef::Id(node.id),
        port: PortRef::Index(port),
        input: Input::Const(value),
    };
    let scalars: &[usize] = match node.kind {
        SignalKind::Gain => &[1],
        SignalKind::Oscillator => &[0, 1],
        SignalKind::Constant => &[0],
        _ => &[],
    };
    if !scalars.is_empty() {
        ui.horizontal(|ui| {
            for &port in scalars {
                let name = node.kind.input_name(port).unwrap_or_default();
                ui.small(name.as_ref());
                match node.input(port) {
                    Some(Input::Const(gradiance_graph::Value::Scalar(mut v))) => {
                        if widgets::config_field(1.0, 0.05).show(ui, &mut v).changed() {
                            edits.push(set(port, gradiance_graph::Value::Scalar(v)));
                        }
                    }
                    _ => {
                        ui.weak("wired");
                    }
                }
            }
        });
    }
    match &node.kind {
        // The only block whose parameter is a shape: the curve editor sits in
        // the block footer, the same place a `k` drag value would.
        SignalKind::Curve => {
            if let Some(Input::Const(gradiance_graph::Value::Curve(mut curve))) = node.input(1)
                && crate::curve::curve_editor(ui, "block-curve", &mut curve)
            {
                edits.push(set(1, gradiance_graph::Value::Curve(curve)));
            }
        }
        SignalKind::Key { key, negative } => {
            let (mut key, mut negative) = (key.clone(), negative.clone());
            let name = node_title(node);
            if key_fields(ui, &name, &mut key, &mut negative, editor_keys) {
                edits.push(GraphEdit::SetKind {
                    node: NodeRef::Id(node.id),
                    kind: SignalKind::Key { key, negative },
                });
            }
        }
        _ => {}
    }
    edits
}

/// A key input's footer (brief N-2): the `+` key, an optional `-` key, and a
/// warning for each editor use of either (in play the input wins; paused,
/// the editor does).
fn key_fields(
    ui: &mut egui::Ui,
    name: &str,
    key: &mut String,
    negative: &mut Option<String>,
    editor_keys: &EditorKeyTable,
) -> bool {
    let mut changed = key_capture(ui, name, "+", key);
    let mut pair = negative.is_some();
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut pair, "pair")
            .on_hover_text("a second key gives -1; both held give 0")
            .changed()
        {
            *negative = pair.then(|| "Q".to_owned());
            changed = true;
        }
    });
    if let Some(negative) = negative.as_mut() {
        changed |= key_capture(ui, name, "-", negative);
    }
    let warning = crate::theme::tokens(ui.ctx()).warning;
    for bound in std::iter::once(&*key).chain(negative.as_ref()) {
        for clash in editor_keys.uses(bound) {
            ui.label(
                egui::RichText::new(format!("{bound} also {clash}: in play this input wins"))
                    .small()
                    .color(warning),
            );
        }
    }
    changed
}

/// One key of a key input: a button that shows the key; click it, then press
/// a key to bind it (Escape cancels). While it listens it claims the
/// keyboard, so the key pressed does not also reach the editor.
fn key_capture(ui: &mut egui::Ui, block: &str, slot: &str, key: &mut String) -> bool {
    let id = egui::Id::new(("graph-key-capture", block, slot));
    let listening = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let mut changed = false;
    let mut answered = false;
    if listening {
        crate::unit_fields::claim_keys(ui.ctx());
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } => Some(*key),
                _ => None,
            })
        });
        if let Some(pressed) = pressed {
            answered = true;
            ui.data_mut(|d| d.remove_temp::<bool>(id));
            if pressed != egui::Key::Escape
                && let Some(named) = gradiance_domain::keys::named_key(pressed.name())
                && named.name != key.as_str()
            {
                named.name.clone_into(key);
                changed = true;
            }
        }
    }
    ui.horizontal(|ui| {
        ui.small(slot);
        let text = if listening && !answered {
            format!("press a key{}", crate::fonts::glyph::ELLIPSIS)
        } else {
            key.clone()
        };
        let response = ui
            .button(text)
            .on_hover_text("click, then press the key to bind (Escape cancels)");
        if response.clicked() && !answered {
            ui.data_mut(|d| d.insert_temp(id, !listening));
        }
    });
    changed
}

/// Rebuilds the snarl wires to mirror the bindings and the signal graph's
/// wires and sensor reads. Called each frame after reconcile so the visible
/// wires always equal the dataflow. Kept separate for unit testing.
fn rebuild_wires(graph: &mut NodeGraph, bindings: &SignalBindings, signal_graph: &Graph) {
    let existing: Vec<(OutPinId, InPinId)> = graph.snarl.wires().collect();
    for (out, in_) in existing {
        graph.snarl.disconnect(out, in_);
    }
    // Binding wires: source output pin → sink input pin.
    for binding in &bindings.0 {
        let Some((src_key, out)) = source_pin(&binding.source, signal_graph) else {
            continue;
        };
        let (sink_key, in_) = sink_pin(&binding.sink);
        if let (Some(&src_id), Some(&sink_id)) = (graph.keys.get(&src_key), graph.keys.get(&sink_key))
        {
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
    // Operand wires: each block pin's binding — a graph wire from another
    // signal node, or a sensor's canonical name read from the bus.
    let operands: Vec<(NodeId, usize, Input)> = graph
        .snarl
        .node_ids()
        .filter_map(|(id, data)| match data {
            NodeData::Computed(node) => Some((id, node)),
            _ => None,
        })
        .flat_map(|(id, node)| {
            operand_ports(&node.kind)
                .into_iter()
                .enumerate()
                .filter_map(move |(pin, port)| node.input(port).map(|input| (id, pin, input)))
                .collect::<Vec<_>>()
        })
        .collect();
    for (block_id, pin, input) in operands {
        let src = match input {
            Input::Wire { node, port } => signal_key(signal_graph, node)
                .and_then(|key| graph.keys.get(&key).copied())
                .map(|id| (id, usize::from(port))),
            Input::External(name) => SignalSource::from_bus_name(&name)
                .and_then(|source| source_pin(&source, signal_graph))
                .and_then(|(key, out)| graph.keys.get(&key).copied().map(|id| (id, out))),
            Input::Const(_) => None,
        };
        if let Some((src_id, out)) = src {
            graph.snarl.connect(
                OutPinId {
                    node: src_id,
                    output: out,
                },
                InPinId {
                    node: block_id,
                    input: pin,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one egui pass of a key input's `+` capture button with `events`;
    /// returns whether the key changed.
    fn capture_pass(ctx: &egui::Context, key: &mut String, events: Vec<egui::Event>) -> bool {
        let mut changed = false;
        let input = egui::RawInput {
            events,
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            changed = key_capture(ui, "in-1", "+", key);
        });
        output.textures_delta.clear();
        changed
    }

    /// A key going down and, in the same pass, up again (egui reads a key
    /// still down from an earlier pass as a repeat).
    fn key_down(key: egui::Key) -> Vec<egui::Event> {
        [true, false]
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            })
            .to_vec()
    }

    #[test]
    fn a_listening_key_button_binds_the_next_key_and_claims_the_keyboard() {
        let ctx = egui::Context::default();
        let listening = egui::Id::new(("graph-key-capture", "in-1", "+"));
        let mut key = "E".to_owned();
        assert!(!capture_pass(&ctx, &mut key, key_down(egui::Key::W)));
        assert_eq!(key, "E", "not listening: a key is the editor's");
        assert!(!crate::unit_fields::take_key_claim(&ctx));

        ctx.data_mut(|d| d.insert_temp(listening, true));
        assert!(capture_pass(&ctx, &mut key, key_down(egui::Key::W)));
        assert_eq!(key, "W", "the pressed key is bound");
        assert!(
            crate::unit_fields::take_key_claim(&ctx),
            "listening claims the keyboard, so W does not also pick a tool"
        );
        assert_eq!(ctx.data(|d| d.get_temp::<bool>(listening)), None);

        ctx.data_mut(|d| d.insert_temp(listening, true));
        assert!(!capture_pass(&ctx, &mut key, key_down(egui::Key::Escape)));
        assert_eq!(key, "W", "Escape cancels");
        assert_eq!(ctx.data(|d| d.get_temp::<bool>(listening)), None);
    }

    fn body(graph: &mut NodeGraph, id: StableId) {
        reconcile_add(
            graph,
            NodeData::Body {
                id,
                name: None,
                kind: "box",
            },
        );
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
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::Fill(b),
        });
        rebuild_wires(&mut graph, &bindings, &Graph::default());
        assert_eq!(graph.snarl.wires().count(), 1, "one wire for the binding");

        // The wire runs A's speed output (index 0) → B's fill input (index 0).
        let (out, in_) = graph.snarl.wires().next().unwrap();
        assert_eq!(out.output, 0, "speed is output 0");
        assert_eq!(in_.input, 0, "fill is input 0");
        assert_eq!(graph.keys[&GraphKey::Body(a)], out.node);
        assert_eq!(graph.keys[&GraphKey::Body(b)], in_.node);
    }

    fn source_pin_of(source: &SignalSource) -> Option<(GraphKey, usize)> {
        source_pin(source, &Graph::default())
    }

    #[test]
    fn source_and_sink_pins_match_the_port_order() {
        let id = StableId::new();
        // Sensor pins line up with body_sensors / joint_sensors order.
        assert_eq!(
            source_pin_of(&SignalSource::Speed(id)),
            Some((GraphKey::Body(id), 0))
        );
        assert_eq!(
            source_pin_of(&SignalSource::ContactCount(id)),
            Some((GraphKey::Body(id), 5))
        );
        assert_eq!(
            source_pin_of(&SignalSource::Named("x".into())),
            None,
            "a name no node publishes has no block"
        );
        let x = StableId::new();
        let labelled = Graph::from_nodes(vec![
            gradiance_graph::Node::new(
                x,
                SignalKind::Param {
                    value: 0.0,
                    min: 0.0,
                    max: 1.0,
                },
            )
            .labelled("x"),
        ]);
        assert_eq!(
            source_pin(&SignalSource::Named("x".into()), &labelled),
            Some((GraphKey::Param(x), 0))
        );
        assert_eq!(
            source_pin_of(&SignalSource::Distance(id, StableId::new())),
            None
        );
        let angle = joint_sensors(id)
            .iter()
            .position(|(_, s)| *s == SignalSource::JointAngle(id));
        assert_eq!(
            source_pin_of(&SignalSource::JointAngle(id)),
            angle.map(|i| (GraphKey::Joint(id), i)),
            "a joint sensor is a joint block's output (brief N-1)"
        );
        assert_eq!(sink_pin(&SignalSink::Fill(id)), (GraphKey::Body(id), 0));
        assert_eq!(
            sink_pin(&SignalSink::TracerColor(id)),
            (GraphKey::Body(id), 1)
        );
        assert_eq!(
            sink_pin(&SignalSink::FieldStrength(id)),
            (GraphKey::Body(id), 2)
        );
        assert_eq!(
            sink_pin(&SignalSink::GravityScale(id)),
            (GraphKey::Body(id), 3)
        );
        assert_eq!(
            sink_pin(&SignalSink::MotorSpeed(id)),
            (GraphKey::Joint(id), 0)
        );
        assert_eq!(
            sink_pin(&SignalSink::MotorCap(id)),
            (GraphKey::Joint(id), 1)
        );
        assert_eq!(sink_pin(&SignalSink::Plot), (GraphKey::Scope, 0));
    }

    #[test]
    fn a_motor_binding_wires_a_sensor_into_a_joint_block() {
        let (wheel, hinge) = (StableId::new(), StableId::new());
        let mut graph = NodeGraph::default();
        body(&mut graph, wheel);
        reconcile_add(
            &mut graph,
            NodeData::Joint {
                id: hinge,
                kind: "hinge",
                motor: true,
                linear: false,
            },
        );
        let bindings = SignalBindings(vec![SignalBinding {
            name: "w".into(),
            source: SignalSource::Height(wheel),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::MotorSpeed(hinge),
        }]);
        rebuild_wires(&mut graph, &bindings, &Graph::default());
        let wires: Vec<_> = graph.snarl.wires().collect();
        assert_eq!(wires.len(), 1, "one wire");
        let (out, into) = wires[0];
        assert_eq!(graph.keys.get(&GraphKey::Body(wheel)), Some(&out.node));
        assert_eq!(graph.keys.get(&GraphKey::Joint(hinge)), Some(&into.node));
        assert_eq!(into.input, 0, "the motor speed pin");
        let joint = graph.snarl.get_node(into.node).unwrap();
        assert_eq!(joint.input_sink(1), Some(SignalSink::MotorCap(hinge)));
        // A joint without a motor offers no actuator pins.
        let bare = NodeData::Joint {
            id: hinge,
            kind: "spring",
            motor: false,
            linear: false,
        };
        assert_eq!(bare.input_sink(0), None);
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
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::Plot,
        });
        rebuild_wires(&mut graph, &bindings, &Graph::default());
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
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::Fill(b),
        });
        let values = binding_input_values(&bindings, &bus);
        // Body b's fill input (index 0) carries the "wire" value.
        assert_eq!(values[&GraphKey::Body(b)][0], Some(42.0));
    }

    #[test]
    fn fresh_labels_avoid_collisions() {
        assert_eq!(fresh_label(&[], "param"), "param-1");
        assert_eq!(fresh_label(&["param-1".to_owned()], "param"), "param-2");
    }

    #[test]
    fn zoom_to_fit_centers_the_graph_and_clamps_the_scale() {
        let view = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 300.0));
        // No nodes → nothing to fit.
        assert!(fit_transform(&[], view).is_none());

        // Two blocks; the fit maps their (expanded) bbox centre to the view
        // centre and never exceeds snarl's zoom range.
        let positions = [egui::pos2(-100.0, -50.0), egui::pos2(300.0, 150.0)];
        let t = fit_transform(&positions, view).expect("some nodes → a transform");
        let bb_center = egui::pos2(100.0, 50.0); // midpoint of the two
        let mapped = t.mul_pos(bb_center);
        assert!(
            (mapped - view.center()).length() < 0.01,
            "bbox centre → view centre"
        );
        assert!(
            (MIN_SCALE..=MAX_SCALE).contains(&t.scaling),
            "scale clamped to snarl's range ({})",
            t.scaling
        );
    }

    #[test]
    fn an_unnamed_body_block_titles_by_its_shape_kind() {
        let id = StableId::new();
        let unnamed = NodeData::Body {
            id,
            name: None,
            kind: "circle",
        };
        assert_eq!(unnamed.title(), "circle", "falls back to the shape kind");
        let named = NodeData::Body {
            id,
            name: Some("wheel".into()),
            kind: "circle",
        };
        assert_eq!(named.title(), "wheel", "a custom name wins");
    }

    #[test]
    fn a_new_wire_name_never_collides_with_an_existing_binding() {
        // A binding's name is its bus name; a duplicate would clobber the
        // other wire's published value, so names must be unique.
        let id = StableId::new();
        let existing = SignalBinding {
            name: "wire-1".into(),
            source: SignalSource::Speed(id),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::Plot,
        };
        let viewer = GraphViewer {
            bindings: vec![existing],
            ..GraphViewer::default()
        };
        // The count-based scheme would also pick "wire-1" here; the fresh name
        // must skip it.
        let name = viewer.fresh_wire_name();
        assert_ne!(name, "wire-1", "must not reuse the taken name");
        assert_eq!(name, "wire-2");
    }

    #[test]
    fn a_modulation_block_operand_wires_to_its_producer_by_id() {
        use gradiance_graph::Node as N;
        let (amp, gain) = (StableId::new(), StableId::new());
        let signal_graph = Graph::from_nodes(vec![
            N::new(
                amp,
                SignalKind::Param {
                    value: 1.0,
                    min: 0.0,
                    max: 2.0,
                },
            )
            .labelled("amp"),
            N::new(gain, SignalKind::Gain)
                .labelled("gain-1")
                .with_inputs(vec![Input::wire(amp), Input::scalar(2.0)]),
        ]);
        let mut graph = NodeGraph::default();
        for node in &signal_graph.nodes {
            let data = match node.kind {
                SignalKind::Param { .. } => NodeData::Param(node.clone()),
                _ => NodeData::Computed(node.clone()),
            };
            reconcile_add(&mut graph, data);
        }
        rebuild_wires(&mut graph, &SignalBindings::default(), &signal_graph);
        assert_eq!(graph.snarl.wires().count(), 1, "operand wire drawn");
        let (out, in_) = graph.snarl.wires().next().unwrap();
        assert_eq!(graph.keys[&GraphKey::Param(amp)], out.node);
        assert_eq!(graph.keys[&GraphKey::Computed(gain)], in_.node);
        assert_eq!(in_.input, 0, "the gain's single operand pin");
    }

    #[test]
    fn a_block_sits_at_its_saved_position_and_follows_a_new_one() {
        use gradiance_graph::Node as N;
        let id = StableId::new();
        let mut node = N::new(id, SignalKind::Time).labelled("clock");
        node.pos = Vec2::new(40.0, 50.0);
        let mut graph = NodeGraph::default();
        reconcile(&mut graph, vec![NodeData::Computed(node.clone())]);
        let snarl_id = graph.keys[&GraphKey::Computed(id)];
        let pos = |g: &NodeGraph| g.snarl.get_node_info(snarl_id).map(|n| n.pos);
        assert_eq!(pos(&graph), Some(egui::pos2(40.0, 50.0)));
        // A drag in progress (the snarl moved, the graph did not) holds.
        if let Some(info) = graph.snarl.get_node_info_mut(snarl_id) {
            info.pos = egui::pos2(90.0, 90.0);
        }
        reconcile(&mut graph, vec![NodeData::Computed(node.clone())]);
        assert_eq!(pos(&graph), Some(egui::pos2(90.0, 90.0)), "not snapped back");
        // A new saved position (an undo, a graph-set) moves the block.
        node.pos = Vec2::new(7.0, 8.0);
        reconcile(&mut graph, vec![NodeData::Computed(node)]);
        assert_eq!(pos(&graph), Some(egui::pos2(7.0, 8.0)));
    }
}
