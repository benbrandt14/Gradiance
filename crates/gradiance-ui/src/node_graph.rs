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
//! — never merely selected, so the canvas stays uncluttered — plus every param
//! and computed signal, keyed by `GraphKey` so dragged positions persist; the
//! wires are rebuilt from the bindings. It docks to the screen bottom.
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
use gradiance_domain::laser::DISTANCE_SUFFIX;
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance_domain::quantity::Subject;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::signal::Subjects;
use gradiance_domain::{Body, Joint};
use gradiance_interaction::graph_keys::EditorKeyTable;
use gradiance_interaction::selection::Selection;
use gradiance_physics::queries::PhysicsQueries;
use gradiance_signal::{
    BlockOp, ComputedSignals, GradientSpec, SignalBinding, SignalBindings, SignalBus, SignalMap,
    SignalParam, SignalParams, SignalSink, SignalSource, read_source,
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
    /// A behavior node (a thruster's thrust pin, a laser's hit and distance
    /// pins), by id (briefs N-3, Q-4).
    Node(StableId),
    /// A `defparam` knob, by name.
    Param(String),
    /// A `defsignal` modulator, by name.
    Computed(String),
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
    /// A laser block (brief Q-4): no inputs, two outputs, its hit (`<name>`,
    /// 0/1) and its distance (`<name>.distance`, m), read off the bus. Shown
    /// while a wire reads it, its body is on the canvas, or it was added.
    Laser { id: StableId, name: String },
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
            Self::Thruster { id } | Self::Laser { id, .. } => GraphKey::Node(*id),
            Self::Param(name) => GraphKey::Param(name.clone()),
            Self::Computed { name, .. } => GraphKey::Computed(name.clone()),
            Self::Scope => GraphKey::Scope,
            Self::Comment { id, .. } => GraphKey::Comment(*id),
        }
    }

    fn role(&self) -> Role {
        match self {
            Self::Body { .. } | Self::Joint { .. } | Self::Thruster { .. } => Role::Body,
            // A laser only produces signals: the producer column, so its
            // wires run left to right into the blocks they drive.
            Self::Param(_) | Self::Laser { .. } => Role::Producer,
            Self::Computed { .. } => Role::Modulator,
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
            Self::Laser { name, .. } => format!("laser {name}"),
            Self::Param(name) => format!("⊙ {name}"),
            Self::Computed { name, .. } => format!("ƒ {name}"),
            Self::Scope => "▭ Scope".to_owned(),
            Self::Comment { .. } => "note".to_owned(),
        }
    }

    /// The [`SignalSource`] a given output pin publishes (for wiring).
    fn output_source(&self, index: usize) -> Option<SignalSource> {
        match self {
            Self::Body { id, .. } => body_sensors(*id).get(index).map(|(_, s)| s.clone()),
            Self::Joint { id, .. } => joint_sensors(*id).get(index).map(|(_, s)| s.clone()),
            Self::Param(name) => (index == 0).then(|| SignalSource::Named(name.clone())),
            Self::Computed { name, .. } => (index == 0).then(|| SignalSource::Named(name.clone())),
            Self::Laser { name, .. } => laser_outputs(name)
                .into_iter()
                .nth(index)
                .map(|(_, bus)| SignalSource::Named(bus)),
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
    /// Bodies, joints and nodes explicitly added to the canvas ("Add to node
    /// editor"). Objects don't auto-appear on selection — only when added or
    /// when a binding references them — so the canvas stays uncluttered.
    added: HashSet<StableId>,
    /// The live lasers by bus name, refreshed each frame: a `Named` source
    /// reading `<name>` or `<name>.distance` is that laser's pin (brief Q-4).
    lasers: HashMap<String, StableId>,
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
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            open: false,
            snarl: Snarl::new(),
            keys: HashMap::new(),
            added: HashSet::new(),
            lasers: HashMap::new(),
            names: HashMap::new(),
            comments: Vec::new(),
            next_comment: 0,
            fit_requested: false,
            last_transform: None,
        }
    }
}

crate::impl_panel_toggle!(NodeGraph, open);

impl NodeGraph {
    /// Adds a body's block to the canvas and opens it (the "Add to node
    /// editor" context action).
    pub fn add_body(&mut self, id: StableId) {
        self.add(id);
    }

    /// Adds a body's, joint's or behaviour node's block to the canvas and
    /// opens it (brief Q-4: the joint and node "Add to node editor" rows).
    pub fn add(&mut self, id: StableId) {
        self.added.insert(id);
        self.open = true;
    }
}

/// The `(GraphKey, output index)` a binding source resolves to, if it maps to
/// a producer block on the canvas: a param or computed block by name, a body
/// sensor in [`body_sensors`] order, a joint sensor in [`joint_sensors`]
/// order. `None` for a two-body source (distance), which has no block.
fn source_pin(
    source: &SignalSource,
    lasers: &HashMap<String, StableId>,
) -> Option<(GraphKey, usize)> {
    match (source, source.reading()) {
        (SignalSource::Named(name), _) => Some(laser_pin(name, lasers).map_or_else(
            || (GraphKey::Param(name.clone()), 0),
            |(id, pin)| (GraphKey::Node(id), pin),
        )),
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

/// A laser's output pins: the label and the bus name each reads.
fn laser_outputs(name: &str) -> [(&'static str, String); 2] {
    [
        ("hit", name.to_owned()),
        ("distance", format!("{name}{DISTANCE_SUFFIX}")),
    ]
}

/// The live laser and output pin a bus name reads: `<name>` is a laser's hit
/// (pin 0), `<name>.distance` its distance (pin 1).
fn laser_pin(bus: &str, lasers: &HashMap<String, StableId>) -> Option<(StableId, usize)> {
    if let Some(&id) = lasers.get(bus) {
        return Some((id, 0));
    }
    let name = bus.strip_suffix(DISTANCE_SUFFIX)?;
    lasers.get(name).map(|&id| (id, 1))
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
    pub(crate) params: ResMut<'w, SignalParams>,
    pub(crate) computed: ResMut<'w, ComputedSignals>,
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
    gp.graph.lasers = gp
        .nodes
        .iter()
        .filter_map(|(id, kind, _)| match kind {
            NodeKind::Laser(laser) => Some((laser.name.clone(), *id)),
            NodeKind::Thruster(_) | NodeKind::Tracer(_) => None,
        })
        .collect();
    let desired = collect_desired(
        &gp.graph.added,
        &gp.graph.lasers,
        &gp.graph.names,
        &gp.graph.comments,
        &gp.bindings,
        &gp.params,
        &gp.computed,
        &gp.bodies,
        &gp.joints,
        &gp.nodes,
    );
    reconcile(&mut gp.graph, desired);
    rebuild_wires(&mut gp.graph, &gp.bindings);

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
        params: gp.params.0.clone(),
        editor_keys: gp.editor_keys.clone(),
        added: gp.graph.added.clone(),
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
    apply_viewer_edits(
        viewer,
        &mut gp.graph,
        &mut gp.bindings,
        &mut gp.params,
        &mut gp.computed,
    );
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
    for (name, value) in viewer.param_edits {
        if let Some(param) = params.0.iter_mut().find(|p| p.name == name) {
            param.value = value;
        }
    }
    if viewer.add_param {
        let name = fresh_param_name(params);
        params.0.push(gradiance_signal::SignalParam::unit(name));
    }
    for op in viewer.add_blocks {
        let name = fresh_block_name(&op, computed);
        computed
            .0
            .push(gradiance_signal::ComputedSignal::from_block(name, op));
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
            NodeData::Param(name) | NodeData::Computed { name, .. } => vec![bus.get(name)],
            NodeData::Laser { name, .. } => laser_outputs(name)
                .iter()
                .map(|(_, bus_name)| bus.get(bus_name))
                .collect(),
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
    lasers: &HashMap<String, StableId>,
    names: &HashMap<StableId, String>,
    comments: &[(u32, String)],
    bindings: &SignalBindings,
    params: &SignalParams,
    computed: &ComputedSignals,
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
        if let Some((GraphKey::Joint(id), _)) = source_pin(&binding.source, lasers) {
            shown_joints.insert(id);
        }
        if let (GraphKey::Joint(id), _) = sink_pin(&binding.sink) {
            shown_joints.insert(id);
        }
    }
    // Bodies and joints feeding a modulation block (a canonical sensor name
    // in its expr).
    for signal in &computed.0 {
        for input in signal.expr.inputs() {
            match SignalSource::from_bus_name(&input).and_then(|s| source_pin(&s, lasers)) {
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
    // A joint block for every joint a wire names, and for every motorised
    // hinge or slider on a body already on the canvas: its motor pins are
    // where a drive is wired (brief N-1).
    for (id, def) in joints {
        let motor = joint_motor(def);
        let on_canvas = def.referenced_bodies().any(|b| shown.contains(&b));
        if shown_joints.contains(id) || added.contains(id) || (motor.is_some() && on_canvas) {
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
    node_blocks(&mut items, added, lasers, &shown, bindings, computed, nodes);
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

/// The behaviour-node blocks (briefs N-3, Q-4): a thruster block for every
/// thruster a wire drives, a laser block for every laser a wire or a
/// modulation block reads, and either for a node that was added or rides a
/// body already on the canvas.
fn node_blocks(
    items: &mut Vec<NodeData>,
    added: &HashSet<StableId>,
    lasers: &HashMap<String, StableId>,
    shown: &HashSet<StableId>,
    bindings: &SignalBindings,
    computed: &ComputedSignals,
    nodes: &Query<(&StableId, &NodeKind, &NodeAttachment), With<BehaviorNode>>,
) {
    // A laser block for every laser a wire or a modulation block reads
    // (brief Q-4), and the same rules as a thruster otherwise.
    let mut read_nodes: HashSet<StableId> = HashSet::new();
    for binding in &bindings.0 {
        if let Some((GraphKey::Node(id), _)) = source_pin(&binding.source, lasers) {
            read_nodes.insert(id);
        }
    }
    for signal in &computed.0 {
        for input in signal.expr.inputs() {
            if let Some((id, _)) = laser_pin(&input, lasers) {
                read_nodes.insert(id);
            }
        }
    }
    for (id, kind, attachment) in nodes {
        let on_canvas = attachment.target.is_some_and(|b| shown.contains(&b));
        let wanted = on_canvas || added.contains(id);
        match kind {
            NodeKind::Thruster(_) => {
                let named = bindings
                    .0
                    .iter()
                    .any(|b| sink_pin(&b.sink).0 == GraphKey::Node(*id));
                if named || wanted {
                    items.push(NodeData::Thruster { id: *id });
                }
            }
            NodeKind::Laser(laser) => {
                if read_nodes.contains(id) || wanted {
                    items.push(NodeData::Laser {
                        id: *id,
                        name: laser.name.clone(),
                    });
                }
            }
            NodeKind::Tracer(_) => {}
        }
    }
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
    /// A snapshot of the params, so a param block's footer can tune its value
    /// in place (the same knob the Signals dock exposes).
    params: Vec<SignalParam>,
    /// Param value edits `(name, new value)` from a param block's footer.
    param_edits: Vec<(String, f32)>,
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
    /// In-canvas authoring requests (applied by the system).
    add_param: bool,
    /// Add a computed/modulation block from a template op.
    add_blocks: Vec<BlockOp>,
    /// Replace a computed block's op (operand wiring + footer constants).
    block_edits: Vec<(String, BlockOp)>,
    /// Objects added by "Add to node editor" (a joint or node block offers
    /// "Remove from node editor" only then).
    added: HashSet<StableId>,
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
            if ui
                .button("Key")
                .on_hover_text("a held key in play: E gives +1, Q gives -1 (set them in the block)")
                .clicked()
            {
                self.add_blocks.push(BlockOp::Key {
                    key: "E".to_owned(),
                    negative: Some("Q".to_owned()),
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
            add(
                ui,
                "ƒ Curve (shape in)",
                BlockOp::Curve {
                    input: None,
                    curve: gradiance_signal::Curve::default(),
                },
            );
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
            NodeData::Param(_) | NodeData::Comment { .. } | NodeData::Laser { .. } => 0,
            NodeData::Computed { inputs, block, .. } => block
                .as_ref()
                .map_or(inputs.len(), |op| op.input_slots().len()),
            NodeData::Scope => 1,
        }
    }

    fn outputs(&mut self, node: &NodeData) -> usize {
        match node {
            NodeData::Body { id, .. } => body_sensors(*id).len(),
            NodeData::Joint { id, .. } => joint_sensors(*id).len(),
            NodeData::Param(_) | NodeData::Computed { .. } => 1,
            NodeData::Laser { .. } => 2,
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
            Some(NodeData::Computed {
                inputs,
                block: None,
                ..
            }) => (inputs.get(pin.id.input).cloned().unwrap_or_default(), ""),
            Some(NodeData::Computed {
                block: Some(op), ..
            }) => (
                op.input_slots()
                    .get(pin.id.input)
                    .map_or_else(String::new, |(l, _)| (*l).to_owned()),
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
            Some(NodeData::Param(name) | NodeData::Computed { name, .. }) => (name.clone(), ""),
            Some(NodeData::Laser { .. }) => match pin.id.output {
                0 => ("hit".to_owned(), ""),
                _ => (
                    "distance".to_owned(),
                    gradiance_units::Dimension::Length.symbol(),
                ),
            },
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
            Some(NodeData::Comment { id, .. }) => {
                let id = *id;
                if ui.button("Delete note").clicked() {
                    self.delete_comments.push(id);
                    ui.close();
                }
            }
            Some(
                NodeData::Joint { id, .. } | NodeData::Thruster { id } | NodeData::Laser { id, .. },
            ) => {
                let id = *id;
                if self.added.contains(&id) {
                    if ui.button("Remove from node editor").clicked() {
                        self.remove_bodies.push(id);
                        ui.close();
                    }
                } else {
                    widgets::empty_state(ui, "shown while a wire or a body on the canvas names it");
                }
            }
            Some(NodeData::Scope) | None => {
                widgets::empty_state(ui, "the plot output");
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
            if block_constants(ui, &name, &mut op, &self.editor_keys) {
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

/// Edits a modulation block's scalar constants in its footer (a key input's
/// keys); returns whether anything changed.
fn block_constants(
    ui: &mut egui::Ui,
    name: &str,
    op: &mut BlockOp,
    editor_keys: &EditorKeyTable,
) -> bool {
    let mut changed = false;
    match op {
        BlockOp::Gain { k, .. } => {
            ui.horizontal(|ui| {
                ui.small("k");
                changed |= widgets::config_field(1.0, 0.05).show(ui, k).changed();
            });
        }
        BlockOp::Oscillator { amp, freq } => {
            ui.horizontal(|ui| {
                ui.small("amp");
                changed |= widgets::config_field(1.0, 0.05).show(ui, amp).changed();
                ui.small("freq");
                changed |= widgets::config_field(1.0, 0.05).show(ui, freq).changed();
            });
        }
        BlockOp::Constant { value } => {
            ui.horizontal(|ui| {
                ui.small("value");
                changed |= widgets::config_field(1.0, 0.05).show(ui, value).changed();
            });
        }
        // The only block whose parameter is a shape: the curve editor sits in
        // the block footer, the same place a `k` drag value would.
        BlockOp::Curve { curve, .. } => {
            changed |= crate::curve::curve_editor(ui, "block-curve", curve);
        }
        BlockOp::Key { key, negative } => {
            changed |= key_fields(ui, name, key, negative, editor_keys);
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
        let Some((src_key, out)) = source_pin(&binding.source, &graph.lasers) else {
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
                let (key, out) = match laser_pin(&name, &graph.lasers) {
                    Some((id, out)) => (GraphKey::Node(id), out),
                    None => source_pin(&SignalSource::from_bus_name(&name)?, &graph.lasers)?,
                };
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
        rebuild_wires(&mut graph, &bindings);
        assert_eq!(graph.snarl.wires().count(), 1, "one wire for the binding");

        // The wire runs A's speed output (index 0) → B's fill input (index 0).
        let (out, in_) = graph.snarl.wires().next().unwrap();
        assert_eq!(out.output, 0, "speed is output 0");
        assert_eq!(in_.input, 0, "fill is input 0");
        assert_eq!(graph.keys[&GraphKey::Body(a)], out.node);
        assert_eq!(graph.keys[&GraphKey::Body(b)], in_.node);
    }

    /// A laser named `gate` is a block with two outputs; a wire reading
    /// `gate` starts at its pin 0, `gate.distance` at pin 1, and an unrelated
    /// name at a param block (brief Q-4).
    #[test]
    fn a_laser_block_claims_its_bus_names() {
        let laser = StableId::new();
        let lasers = HashMap::from([("gate".to_owned(), laser)]);
        let named = |n: &str| SignalSource::Named(n.into());
        assert_eq!(
            source_pin(&named("gate"), &lasers),
            Some((GraphKey::Node(laser), 0))
        );
        assert_eq!(
            source_pin(&named("gate.distance"), &lasers),
            Some((GraphKey::Node(laser), 1))
        );
        assert_eq!(
            source_pin(&named("gated"), &lasers),
            Some((GraphKey::Param("gated".into()), 0))
        );
        let block = NodeData::Laser {
            id: laser,
            name: "gate".into(),
        };
        assert_eq!(block.output_source(0), Some(named("gate")));
        assert_eq!(block.output_source(1), Some(named("gate.distance")));
        assert_eq!(block.output_source(2), None);

        // A gravity-scale wire from the hit pin draws from the laser block.
        let mut graph = NodeGraph {
            lasers,
            ..NodeGraph::default()
        };
        let ball = StableId::new();
        reconcile_add(&mut graph, block);
        body(&mut graph, ball);
        let mut bindings = SignalBindings::default();
        bindings.0.push(SignalBinding {
            name: "lift".into(),
            source: named("gate.distance"),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::GravityScale(ball),
        });
        rebuild_wires(&mut graph, &bindings);
        let (out, in_) = graph.snarl.wires().next().expect("a wire");
        assert_eq!(graph.keys[&GraphKey::Node(laser)], out.node);
        assert_eq!(out.output, 1, "the distance pin");
        assert_eq!(graph.keys[&GraphKey::Body(ball)], in_.node);
    }

    #[test]
    fn source_and_sink_pins_match_the_port_order() {
        let id = StableId::new();
        // Sensor pins line up with body_sensors / joint_sensors order.
        assert_eq!(
            source_pin(&SignalSource::Speed(id), &HashMap::new()),
            Some((GraphKey::Body(id), 0))
        );
        assert_eq!(
            source_pin(&SignalSource::ContactCount(id), &HashMap::new()),
            Some((GraphKey::Body(id), 5))
        );
        assert_eq!(
            source_pin(&SignalSource::Named("x".into()), &HashMap::new()),
            Some((GraphKey::Param("x".into()), 0))
        );
        assert_eq!(
            source_pin(
                &SignalSource::Distance(id, StableId::new()),
                &HashMap::new()
            ),
            None
        );
        let angle = joint_sensors(id)
            .iter()
            .position(|(_, s)| *s == SignalSource::JointAngle(id));
        assert_eq!(
            source_pin(&SignalSource::JointAngle(id), &HashMap::new()),
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
        rebuild_wires(&mut graph, &bindings);
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
            curve: None,
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
        params
            .0
            .push(gradiance_signal::SignalParam::unit("param-1"));
        assert_eq!(fresh_param_name(&params), "param-2");
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
