//! The IR types: a [`Graph`] of [`Node`]s whose [`Input`]s are constants,
//! wires by id, or external bus reads.

use bevy::math::Vec2;
use bevy::prelude::{Reflect, Resource};
use gradiance_core::ids::StableId;
use gradiance_domain::shape::{CsgOp, ShapeDef};
use gradiance_domain::signal::{Curve, SignalExpr};
use serde::{Deserialize, Serialize};

use crate::catalog::{PortMeta, meta};

/// One graph: every node, sorted by id (the order [`Graph::node`] searches
/// and the tie-break of every deterministic walk).
///
/// Scene content (brief G-2): one resource, saved with the scene and undone
/// as one `settings` step per settled edit.
#[derive(Resource, Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct Graph {
    /// The nodes, sorted by id ([`Graph::validate`] refuses another order).
    pub nodes: Vec<Node>,
}

impl Graph {
    /// A graph of `nodes`, sorted by id.
    pub fn from_nodes(mut nodes: Vec<Node>) -> Self {
        nodes.sort_by_key(|n| n.id.0);
        Self { nodes }
    }

    /// The node with `id`, if any (a binary search: the nodes are sorted).
    pub fn node(&self, id: StableId) -> Option<&Node> {
        self.index_of(id).map(|i| &self.nodes[i])
    }

    /// Index of the node with `id` in [`Graph::nodes`], if any.
    pub fn index_of(&self, id: StableId) -> Option<usize> {
        self.nodes.binary_search_by_key(&id.0, |n| n.id.0).ok()
    }

    /// Inserts `node` in id order, returning the node it replaced.
    pub fn insert(&mut self, node: Node) -> Option<Node> {
        match self.nodes.binary_search_by_key(&node.id.0, |n| n.id.0) {
            Ok(i) => Some(std::mem::replace(&mut self.nodes[i], node)),
            Err(i) => {
                self.nodes.insert(i, node);
                None
            }
        }
    }

    /// The node publishing under `label`, if any.
    pub fn by_label(&self, label: &str) -> Option<&Node> {
        self.nodes
            .iter()
            .find(|n| n.label.as_deref() == Some(label))
    }
}

/// One node: an identity, a kind, its input bindings and where it sits on
/// the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Node {
    /// Identity; every edge names its source by this id.
    pub id: StableId,
    /// What the node computes.
    pub kind: NodeKind,
    /// One binding per input port, in port order. A missing trailing
    /// entry reads the port's default ([`PortMeta::default`]).
    #[serde(default)]
    pub inputs: Vec<Input>,
    /// The bus name a signal node publishes under (`None`: unpublished; a
    /// wire to it inlines its expression).
    #[serde(default)]
    pub label: Option<String>,
    /// Canvas position: saved, and undone with the graph.
    #[serde(default)]
    pub pos: Vec2,
}

impl Node {
    /// A node of `kind` with every input at its default, unlabelled, at the
    /// canvas origin.
    pub fn new(id: StableId, kind: NodeKind) -> Self {
        Self {
            id,
            kind,
            inputs: Vec::new(),
            label: None,
            pos: Vec2::ZERO,
        }
    }

    /// The same node publishing under `label`.
    #[must_use]
    pub fn labelled(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The same node with `inputs` bound in port order.
    #[must_use]
    pub fn with_inputs(mut self, inputs: Vec<Input>) -> Self {
        self.inputs = inputs;
        self
    }

    /// The binding of input port `i`: the authored one, or the port's
    /// default constant when the node leaves it out (`None` past the last
    /// port).
    pub fn input(&self, i: usize) -> Option<Input> {
        if let Some(input) = self.inputs.get(i) {
            return Some(input.clone());
        }
        self.kind
            .input_ports()
            .get(i)
            .map(|p| Input::Const(p.default.value()))
    }
}

/// What feeds one input port.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Input {
    /// A constant (an unconnected port holds one).
    Const(Value),
    /// An edge: output `port` of the node with id `node`.
    Wire {
        /// The source node.
        node: StableId,
        /// The source's output port index.
        port: u8,
    },
    /// A bus value the graph does not produce (`"t"`, a sensor read, a
    /// key's held state, a laser output, a script's `signal-set`). The one
    /// name-addressed input (decision § What it forecloses). Frame rate.
    External(String),
}

impl Input {
    /// A scalar constant.
    pub fn scalar(value: f32) -> Self {
        Self::Const(Value::Scalar(value))
    }

    /// A wire from output 0 of `node`.
    pub fn wire(node: StableId) -> Self {
        Self::Wire { node, port: 0 }
    }
}

/// The type of a port. Open: a later phase adds what it needs (a `Frame`
/// for a sketch plane, F4), never a world axis on an existing type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum PortType {
    /// One `f32`.
    Scalar,
    /// One plane-local point or offset.
    Vec2,
    /// An ordered point list (a polygon outline).
    Points,
    /// A list of point lists (polygon holes).
    Contours,
    /// A shape tree.
    Shape,
    /// A response curve.
    Curve,
}

/// A constant of each [`PortType`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Value {
    /// A [`PortType::Scalar`].
    Scalar(f32),
    /// A [`PortType::Vec2`].
    Vec2(Vec2),
    /// A [`PortType::Points`].
    Points(Vec<Vec2>),
    /// A [`PortType::Contours`].
    Contours(Vec<Vec<Vec2>>),
    /// A [`PortType::Shape`] (a baked shape is a constant).
    Shape(ShapeDef),
    /// A [`PortType::Curve`].
    Curve(Curve),
}

impl Value {
    /// The port type this constant fits.
    pub fn port_type(&self) -> PortType {
        match self {
            Self::Scalar(_) => PortType::Scalar,
            Self::Vec2(_) => PortType::Vec2,
            Self::Points(_) => PortType::Points,
            Self::Contours(_) => PortType::Contours,
            Self::Shape(_) => PortType::Shape,
            Self::Curve(_) => PortType::Curve,
        }
    }
}

/// Which lowering a node belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum Family {
    /// Lowers to a `SignalExpr`, then to the kernel.
    Signal,
    /// Lowers to a `ShapeDef`.
    Shape,
}

/// How often a node's output may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum Rate {
    /// Every frame (the kernel's rate).
    Frame,
    /// Only on a settled edit. An edit-rate input never takes a frame-rate
    /// output.
    Edit,
}

/// What a node computes. Numbers a node reads are input ports (with a
/// default and a `hard` range in its catalog row), so any of them can take
/// a wire; only what is not a port value (key names, a param's own value
/// and bounds, a console expression, a boolean operator) is a field here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum NodeKind {
    /// The elapsed-seconds clock `t`.
    Time,
    /// A fixed value (port `value`).
    Constant,
    /// `amp · sin(freq · t)`.
    Oscillator,
    /// `in · k`.
    Gain,
    /// `a + b`.
    Sum,
    /// `a · b`.
    Product,
    /// `a − b`.
    Sub,
    /// `|in|`.
    Abs,
    /// `min(a, b)`.
    Min,
    /// `max(a, b)`.
    Max,
    /// `curve(in)`: reshape through a response curve (port `curve`).
    Curve,
    /// A key's held state: `1` while `key` is held (`+1`/`-1`/`0` with a
    /// `negative` key). Reads the external `key@<name>` bus inputs.
    Key {
        /// The key that drives the output to `+1`.
        key: String,
        /// The key that drives it to `-1`.
        negative: Option<String>,
    },
    /// A tunable parameter (`defparam`): an edit-rate scalar that also
    /// publishes each frame under its label.
    Param {
        /// Current value.
        value: f32,
        /// Slider lower bound.
        min: f32,
        /// Slider upper bound.
        max: f32,
    },
    /// A console `defsignal` expression. Its named inputs are its ports,
    /// in first-appearance order ([`SignalExpr::inputs`]); a binding
    /// replaces every read of that name.
    Expr(SignalExpr),
    /// A box (ports `width`, `height`).
    Box,
    /// A circle (port `radius`).
    Circle,
    /// A polygon (ports `outline`, `holes`).
    Polygon,
    /// A capsule (ports `half_length`, `radius`).
    Capsule,
    /// The ground half-plane (no ports).
    HalfPlane,
    /// A boolean of ports `lhs` and `rhs`.
    Csg(CsgOp),
    /// A subtree (port `shape`) placed at `pos`, rotated `rot`.
    Placed,
}

impl NodeKind {
    /// The input ports of this kind: its catalog row's, except for an
    /// [`Expr`](NodeKind::Expr), which has one scalar port per name its
    /// expression reads.
    pub fn input_ports(&self) -> std::borrow::Cow<'static, [PortMeta]> {
        match self {
            Self::Expr(expr) => {
                std::borrow::Cow::Owned(vec![PortMeta::EXPR_INPUT; expr.inputs().len()])
            }
            other => std::borrow::Cow::Borrowed(meta(other).inputs),
        }
    }

    /// The display name of input port `i`: the catalog's, or for an
    /// [`Expr`](NodeKind::Expr) the bus name its expression reads there.
    pub fn input_name(&self, i: usize) -> Option<std::borrow::Cow<'static, str>> {
        match self {
            Self::Expr(expr) => expr
                .inputs()
                .into_iter()
                .nth(i)
                .map(std::borrow::Cow::Owned),
            other => meta(other)
                .inputs
                .get(i)
                .map(|p| std::borrow::Cow::Borrowed(p.name)),
        }
    }

    /// This kind's family (from its catalog row).
    pub fn family(&self) -> Family {
        meta(self).family
    }

    /// This kind's output rate (from its catalog row).
    pub fn rate(&self) -> Rate {
        meta(self).rate
    }
}
