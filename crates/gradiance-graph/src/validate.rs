//! Validation: port types, rates, labels, cycles and shape depth, and the
//! deterministic evaluation [`Order`].

use std::collections::BTreeSet;

use gradiance_core::ids::StableId;
use gradiance_domain::shape::{MAX_CSG_DEPTH, ShapeError};

use crate::catalog::meta;
use crate::ir::{Family, Graph, Input, NodeKind, PortType, Rate, Value};

/// Why a graph (or one lowering) was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GraphError {
    /// The nodes are not sorted by id.
    #[error("graph nodes are not sorted by id (at node {0})")]
    Unsorted(StableId),
    /// Two nodes share an id.
    #[error("two graph nodes share the id {0}")]
    DuplicateNode(StableId),
    /// A lowering was asked for a node the graph does not have.
    #[error("the graph has no node {0}")]
    UnknownNode(StableId),
    /// A wire names a node the graph does not have.
    #[error("input {input} of node {node} is wired to a missing node {from}")]
    UnknownSource {
        /// The node whose input is wired.
        node: StableId,
        /// The input port index.
        input: usize,
        /// The missing source id.
        from: StableId,
    },
    /// A node binds more inputs than its kind has ports.
    #[error("node {node} has no input port {input}")]
    UnknownInput {
        /// The node.
        node: StableId,
        /// The first binding past the last port.
        input: usize,
    },
    /// A wire names an output port its source does not have.
    #[error("input {input} of node {node} is wired to output {port}, which its source lacks")]
    UnknownOutput {
        /// The node whose input is wired.
        node: StableId,
        /// The input port index.
        input: usize,
        /// The missing output port.
        port: u8,
    },
    /// A constant or a wire does not have the port's type.
    #[error("input {input} of node {node} takes {expected:?} but gets {found:?}")]
    TypeMismatch {
        /// The node.
        node: StableId,
        /// The input port index.
        input: usize,
        /// The port's type.
        expected: PortType,
        /// What was bound to it.
        found: PortType,
    },
    /// An edit-rate input takes a frame-rate output (a wire from a
    /// frame-rate node, or an external bus read).
    #[error("input {input} of edit-rate node {node} takes a frame-rate value")]
    RateMismatch {
        /// The edit-rate node.
        node: StableId,
        /// The input port index.
        input: usize,
    },
    /// A scalar constant lies outside its port's `hard` range.
    #[error("input {input} of node {node} is {value}, outside its hard range")]
    OutOfRange {
        /// The node.
        node: StableId,
        /// The input port index.
        input: usize,
        /// The constant.
        value: f32,
    },
    /// Two nodes publish under one label.
    #[error("two graph nodes publish under the label {0:?}")]
    DuplicateLabel(String),
    /// A cycle passes through no labelled node, so no bus value can carry
    /// last frame's value round it.
    #[error("node {0} is on a cycle of unlabelled nodes")]
    UnlabelledCycle(StableId),
    /// A cycle of edit-rate nodes (a shape that contains itself).
    #[error("edit-rate node {0} is on a cycle")]
    EditCycle(StableId),
    /// A shape subgraph deeper than `MAX_CSG_DEPTH`.
    #[error("shape node {node} is {depth} deep (at most {MAX_CSG_DEPTH})")]
    TooDeep {
        /// The shape node.
        node: StableId,
        /// Its tree depth.
        depth: u32,
    },
    /// `lower_signal` was asked for a node of another family.
    #[error("node {0} is not a signal node")]
    NotSignal(StableId),
    /// `lower_shape` was asked for a node of another family.
    #[error("node {0} is not a shape node")]
    NotShape(StableId),
    /// The lowered shape fails `ShapeDef::validate`.
    #[error("shape node {node} lowers to an invalid shape: {error}")]
    Shape {
        /// The root of the lowering.
        node: StableId,
        /// Why the shape was refused.
        error: ShapeError,
    },
}

/// An edge dropped to break a frame-rate cycle: `to` reads `from`'s value
/// of the previous frame (through the bus, as today).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feedback {
    /// The source node.
    pub from: StableId,
    /// The node whose input reads last frame's value.
    pub to: StableId,
    /// That input's port index.
    pub input: usize,
}

/// A deterministic evaluation order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Order {
    /// Every node, each after its sources (ties by id), feedback edges
    /// aside.
    pub nodes: Vec<StableId>,
    /// The edges dropped to break frame-rate cycles. Each cycle is broken
    /// at its edges into its lowest-id node.
    pub feedback: Vec<Feedback>,
}

/// One wire, by node index.
#[derive(Debug, Clone, Copy)]
struct Edge {
    from: usize,
    to: usize,
    input: usize,
}

impl Graph {
    /// Validates the whole graph and returns its evaluation order.
    ///
    /// Checks, in this order: ids sorted and unique; every binding against
    /// its port (known port, known source and output, type, rate, `hard`
    /// range); labels unique; no cycle through unlabelled nodes only; no
    /// edit-rate cycle; no shape deeper than `MAX_CSG_DEPTH`. A frame-rate
    /// cycle is not an error: it is broken at the edges into its lowest-id
    /// node and reported in [`Order::feedback`].
    pub fn validate(&self) -> Result<Order, GraphError> {
        self.check_ids()?;
        let edges = self.check_bindings()?;
        self.check_labels()?;
        self.check_unlabelled_cycles(&edges)?;
        let order = self.order(&edges)?;
        self.check_depth(&order)?;
        Ok(order)
    }

    fn check_ids(&self) -> Result<(), GraphError> {
        for pair in self.nodes.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            match a.id.0.cmp(&b.id.0) {
                std::cmp::Ordering::Less => {}
                std::cmp::Ordering::Equal => return Err(GraphError::DuplicateNode(b.id)),
                std::cmp::Ordering::Greater => return Err(GraphError::Unsorted(b.id)),
            }
        }
        Ok(())
    }

    fn check_bindings(&self) -> Result<Vec<Edge>, GraphError> {
        let mut edges = Vec::new();
        for (to, node) in self.nodes.iter().enumerate() {
            let ports = node.kind.input_ports();
            if node.inputs.len() > ports.len() {
                return Err(GraphError::UnknownInput {
                    node: node.id,
                    input: ports.len(),
                });
            }
            let edit = node.kind.rate() == Rate::Edit;
            for (input, (binding, port)) in node.inputs.iter().zip(ports.iter()).enumerate() {
                let mismatch = |found| GraphError::TypeMismatch {
                    node: node.id,
                    input,
                    expected: port.ty,
                    found,
                };
                match binding {
                    Input::Const(value) => {
                        if value.port_type() != port.ty {
                            return Err(mismatch(value.port_type()));
                        }
                        if let (Value::Scalar(v), Some((lo, hi))) = (value, port.hard)
                            && !(lo..=hi).contains(v)
                        {
                            return Err(GraphError::OutOfRange {
                                node: node.id,
                                input,
                                value: *v,
                            });
                        }
                    }
                    Input::External(_) => {
                        if port.ty != PortType::Scalar {
                            return Err(mismatch(PortType::Scalar));
                        }
                        if edit {
                            return Err(GraphError::RateMismatch {
                                node: node.id,
                                input,
                            });
                        }
                    }
                    Input::Wire {
                        node: source,
                        port: out,
                    } => {
                        let from = self.index_of(*source).ok_or(GraphError::UnknownSource {
                            node: node.id,
                            input,
                            from: *source,
                        })?;
                        let source_meta = meta(&self.nodes[from].kind);
                        let output = source_meta.outputs.get(usize::from(*out)).ok_or(
                            GraphError::UnknownOutput {
                                node: node.id,
                                input,
                                port: *out,
                            },
                        )?;
                        if output.ty != port.ty {
                            return Err(mismatch(output.ty));
                        }
                        if edit && source_meta.rate == Rate::Frame {
                            return Err(GraphError::RateMismatch {
                                node: node.id,
                                input,
                            });
                        }
                        edges.push(Edge { from, to, input });
                    }
                }
            }
        }
        Ok(edges)
    }

    fn check_labels(&self) -> Result<(), GraphError> {
        let mut seen = BTreeSet::new();
        for label in self.nodes.iter().filter_map(|n| n.label.as_deref()) {
            if !seen.insert(label) {
                return Err(GraphError::DuplicateLabel(label.to_owned()));
            }
        }
        Ok(())
    }

    /// A signal lowering stops at a labelled source (it reads the bus) and
    /// inlines an unlabelled one, so the wires from unlabelled nodes into
    /// frame-rate nodes must be acyclic. (An edit-rate cycle is refused
    /// whatever its labels: [`GraphError::EditCycle`].)
    fn check_unlabelled_cycles(&self, edges: &[Edge]) -> Result<(), GraphError> {
        let inner: Vec<Edge> = edges
            .iter()
            .copied()
            .filter(|e| {
                self.nodes[e.from].label.is_none() && self.nodes[e.to].kind.rate() == Rate::Frame
            })
            .collect();
        let mut in_degree = vec![0usize; self.nodes.len()];
        for e in &inner {
            in_degree[e.to] += 1;
        }
        let mut ready: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| in_degree[i] == 0)
            .collect();
        let mut done = 0;
        while let Some(v) = ready.pop() {
            done += 1;
            for e in inner.iter().filter(|e| e.from == v) {
                in_degree[e.to] -= 1;
                if in_degree[e.to] == 0 {
                    ready.push(e.to);
                }
            }
        }
        if done == self.nodes.len() {
            return Ok(());
        }
        // Name the lowest-id node on such a cycle (a waiting node may be
        // only downstream of one).
        let stuck = (0..self.nodes.len())
            .find(|&i| in_degree[i] > 0 && reaches(&inner, i, i))
            .unwrap_or(0);
        Err(GraphError::UnlabelledCycle(self.nodes[stuck].id))
    }

    /// Kahn's algorithm with ties by id; when every waiting node waits on
    /// another, the lowest-id node on a cycle has its in-cycle wires
    /// dropped as feedback.
    fn order(&self, edges: &[Edge]) -> Result<Order, GraphError> {
        let n = self.nodes.len();
        let mut active = vec![true; edges.len()];
        let mut in_degree = vec![0usize; n];
        for e in edges {
            in_degree[e.to] += 1;
        }
        // Node indices are id order, so the smallest index is the lowest id.
        let mut ready: BTreeSet<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut placed = vec![false; n];
        let mut order = Order::default();
        while order.nodes.len() < n {
            if let Some(v) = ready.pop_first() {
                placed[v] = true;
                order.nodes.push(self.nodes[v].id);
                for (k, e) in edges.iter().enumerate() {
                    if active[k] && e.from == v {
                        active[k] = false;
                        in_degree[e.to] -= 1;
                        if in_degree[e.to] == 0 {
                            ready.insert(e.to);
                        }
                    }
                }
                continue;
            }
            let live: Vec<Edge> = edges
                .iter()
                .enumerate()
                .filter(|(k, _)| active[*k])
                .map(|(_, e)| *e)
                .collect();
            let Some(c) = (0..n).find(|&i| !placed[i] && reaches(&live, i, i)) else {
                // Unreachable: a stuck walk always has a cycle. Place the
                // rest in id order rather than loop.
                order
                    .nodes
                    .extend((0..n).filter(|&i| !placed[i]).map(|i| self.nodes[i].id));
                break;
            };
            if self.nodes[c].kind.rate() == Rate::Edit {
                return Err(GraphError::EditCycle(self.nodes[c].id));
            }
            for (k, e) in edges.iter().enumerate() {
                if active[k] && e.to == c && reaches(&live, c, e.from) {
                    active[k] = false;
                    in_degree[c] -= 1;
                    order.feedback.push(Feedback {
                        from: self.nodes[e.from].id,
                        to: self.nodes[c].id,
                        input: e.input,
                    });
                }
            }
            if in_degree[c] == 0 {
                ready.insert(c);
            }
        }
        Ok(order)
    }

    fn check_depth(&self, order: &Order) -> Result<(), GraphError> {
        let mut depth = vec![0u32; self.nodes.len()];
        for &id in &order.nodes {
            let Some(i) = self.index_of(id) else { continue };
            let node = &self.nodes[i];
            if node.kind.family() != Family::Shape {
                continue;
            }
            let input_depth = |k: usize| match node.input(k) {
                Some(Input::Const(Value::Shape(s))) => s.depth(),
                Some(Input::Wire { node: src, .. }) => self.index_of(src).map_or(1, |j| depth[j]),
                _ => 1,
            };
            let d = match node.kind {
                NodeKind::Csg(_) => 1 + input_depth(0).max(input_depth(1)),
                NodeKind::Placed => 1 + input_depth(0),
                _ => 1,
            };
            if d > MAX_CSG_DEPTH {
                return Err(GraphError::TooDeep { node: id, depth: d });
            }
            depth[i] = d;
        }
        Ok(())
    }
}

/// Whether `to` is reachable from `from` along at least one of `edges`.
fn reaches(edges: &[Edge], from: usize, to: usize) -> bool {
    let mut stack = vec![from];
    let mut seen = BTreeSet::new();
    while let Some(v) = stack.pop() {
        for e in edges.iter().filter(|e| e.from == v) {
            if e.to == to {
                return true;
            }
            if seen.insert(e.to) {
                stack.push(e.to);
            }
        }
    }
    false
}
