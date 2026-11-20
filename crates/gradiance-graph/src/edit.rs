//! Edits: the one way a graph changes. The `graph.*` and `signal.*`
//! operations and the canvas all build a [`GraphEdit`] and apply it with
//! [`Graph::edit`], which validates the result before it keeps it.

use bevy::math::Vec2;
use gradiance_core::ids::StableId;
use gradiance_domain::signal::SignalExpr;

use crate::ir::{Graph, Input, Node, NodeKind};
use crate::validate::GraphError;

/// The bus name of the elapsed-seconds clock, which no node may take as its
/// label (it would shadow the clock every oscillator reads).
const RESERVED_LABEL: &str = "t";

/// How an edit names a node: by id, or by the label it publishes under.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeRef {
    /// The node with this id.
    Id(StableId),
    /// The node labelled this.
    Label(String),
}

impl NodeRef {
    /// Reads a doorway's text: a UUID names an id, anything else a label.
    pub fn parse(text: &str) -> Self {
        text.parse::<StableId>()
            .map_or_else(|_| Self::Label(text.trim().to_owned()), Self::Id)
    }
}

impl std::fmt::Display for NodeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(id) => id.fmt(f),
            Self::Label(label) => write!(f, "{label:?}"),
        }
    }
}

/// How an edit names an input port: by index, or by its name (the catalog
/// name, or for an expression node the bus name its expression reads).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PortRef {
    /// Port `i`, in binding order.
    Index(usize),
    /// The port with this name.
    Name(String),
}

impl std::fmt::Display for PortRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Index(i) => i.fmt(f),
            Self::Name(name) => write!(f, "{name:?}"),
        }
    }
}

/// One change to a graph. Names resolve when the edit is applied, against
/// the graph as it then stands, so a script that declares `amp` and then a
/// signal reading `amp` in one run wires the two (the run's snapshot does
/// not hold `amp` yet; the graph the second edit applies to does).
#[derive(Debug, Clone, PartialEq)]
pub enum GraphEdit {
    /// Declare (or update, by name) a param (`defparam`).
    Param {
        /// The id a new node takes (unused when `name` already names one).
        id: StableId,
        /// The label.
        name: String,
        /// Current value.
        value: f32,
        /// Slider lower bound.
        min: f32,
        /// Slider upper bound.
        max: f32,
    },
    /// Declare (or update, by name) an expression node (`defsignal`). Each
    /// name the expression reads is wired to the node with that label, or
    /// read from the bus when no node has it.
    Expr {
        /// The id a new node takes.
        id: StableId,
        /// The label.
        name: String,
        /// The expression.
        expr: SignalExpr,
    },
    /// Declare (or update, by name) a key node (`defkey`).
    Key {
        /// The id a new node takes.
        id: StableId,
        /// The label.
        name: String,
        /// The `+1` key.
        key: String,
        /// The `-1` key.
        negative: Option<String>,
    },
    /// Add a node (`graph.add`, a canvas palette entry).
    Add {
        /// Its id (refused if taken).
        id: StableId,
        /// Its kind.
        kind: NodeKind,
        /// Its label, if it publishes.
        label: Option<String>,
        /// Bindings in port order (missing ones read the port default).
        inputs: Vec<Input>,
    },
    /// Wire output 0 of `from` into input `port` of `to`.
    Connect {
        /// The source.
        from: NodeRef,
        /// The node whose input is wired.
        to: NodeRef,
        /// The input port.
        port: PortRef,
    },
    /// Bind input `port` of `node` to `input` (a constant, a bus read or a
    /// wire); a constant disconnects the port.
    SetInput {
        /// The node.
        node: NodeRef,
        /// The input port.
        port: PortRef,
        /// The new binding.
        input: Input,
    },
    /// Set or clear a node's label. Wires to it are by id, so they keep
    /// reading it under the new name.
    SetLabel {
        /// The node.
        node: NodeRef,
        /// The new label (`None`: it stops publishing).
        label: Option<String>,
    },
    /// Move a node on the canvas.
    SetPos {
        /// The node.
        node: NodeRef,
        /// The new position.
        pos: Vec2,
    },
    /// Replace a node's kind (a param's value, a key's keys), keeping its
    /// id, label, position and the bindings its new kind still has.
    SetKind {
        /// The node.
        node: NodeRef,
        /// The new kind.
        kind: NodeKind,
    },
    /// Remove a node. A wire from it becomes a bus read of its label, or
    /// the port's default when it had none.
    Remove {
        /// The node.
        node: NodeRef,
    },
}

/// Why an edit was refused. A refused edit leaves the graph unchanged.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditError {
    /// No node has this id or label.
    #[error("the graph has no node {0}")]
    UnknownNode(NodeRef),
    /// The node has no such input port.
    #[error("node {node} has no input port {port}")]
    UnknownPort {
        /// The node.
        node: NodeRef,
        /// The port asked for.
        port: PortRef,
    },
    /// Another node already publishes under this label.
    #[error("another node is labelled {0:?}")]
    LabelTaken(String),
    /// A label is empty, or is the clock's reserved name `t`.
    #[error("{0:?} cannot be a label (empty, or the clock's name \"t\")")]
    BadLabel(String),
    /// An added node's id is taken.
    #[error("the id {0} already names a node")]
    IdTaken(StableId),
    /// The edited graph does not validate.
    #[error(transparent)]
    Invalid(#[from] GraphError),
}

impl Graph {
    /// Applies `edit` and validates the result; on any error the graph is
    /// left as it was. Returns the id of the node the edit touched.
    pub fn edit(&mut self, edit: &GraphEdit) -> Result<StableId, EditError> {
        let mut next = self.clone();
        let id = next.apply(edit)?;
        next.validate()?;
        *self = next;
        Ok(id)
    }

    /// The id `node` names, if any.
    pub fn resolve(&self, node: &NodeRef) -> Option<StableId> {
        match node {
            NodeRef::Id(id) => self.node(*id).map(|n| n.id),
            NodeRef::Label(label) => self.by_label(label).map(|n| n.id),
        }
    }

    /// The node with `id`, mutably.
    pub fn node_mut(&mut self, id: StableId) -> Option<&mut Node> {
        self.index_of(id).map(|i| &mut self.nodes[i])
    }

    /// Removes the node with `id`. Every wire from it becomes a bus read of
    /// its label (the by-name read it stood for), or the port's default
    /// constant when it had no label.
    pub fn remove(&mut self, id: StableId) -> Option<Node> {
        let removed = self.nodes.remove(self.index_of(id)?);
        for node in &mut self.nodes {
            let ports = node.kind.input_ports();
            for (i, input) in node.inputs.iter_mut().enumerate() {
                if matches!(input, Input::Wire { node: from, .. } if *from == id) {
                    *input = match &removed.label {
                        Some(label) => Input::External(label.clone()),
                        None => ports
                            .get(i)
                            .map_or(Input::scalar(0.0), |p| Input::Const(p.default.value())),
                    };
                }
            }
        }
        Some(removed)
    }

    /// Where a new node of `kind` goes on the canvas: below the last node of
    /// its column (sources left, the rest right of them).
    pub fn free_pos(&self, kind: &NodeKind) -> Vec2 {
        const TOP: f32 = 16.0;
        const PITCH: f32 = 112.0;
        let x = column_x(kind);
        let y = self
            .nodes
            .iter()
            .filter(|n| n.pos.x == x)
            .map(|n| n.pos.y + PITCH)
            .fold(TOP, f32::max);
        Vec2::new(x, y)
    }

    fn apply(&mut self, edit: &GraphEdit) -> Result<StableId, EditError> {
        match edit {
            GraphEdit::Param {
                id,
                name,
                value,
                min,
                max,
            } => {
                let kind = NodeKind::Param {
                    value: *value,
                    min: *min,
                    max: *max,
                };
                self.declare(*id, name, kind, |_, _| Vec::new())
            }
            GraphEdit::Expr { id, name, expr } => {
                self.declare(*id, name, NodeKind::Expr(expr.clone()), |graph, _| {
                    expr.inputs()
                        .into_iter()
                        .map(|read| match graph.by_label(&read) {
                            Some(source) => Input::wire(source.id),
                            None => Input::External(read),
                        })
                        .collect()
                })
            }
            GraphEdit::Key {
                id,
                name,
                key,
                negative,
            } => {
                let kind = NodeKind::Key {
                    key: key.clone(),
                    negative: negative.clone(),
                };
                self.declare(*id, name, kind, |_, _| Vec::new())
            }
            GraphEdit::Add {
                id,
                kind,
                label,
                inputs,
            } => {
                if self.node(*id).is_some() {
                    return Err(EditError::IdTaken(*id));
                }
                if let Some(label) = label {
                    self.check_label(label, None)?;
                }
                let pos = self.free_pos(kind);
                self.insert(Node {
                    id: *id,
                    kind: kind.clone(),
                    inputs: inputs.clone(),
                    label: label.clone(),
                    pos,
                });
                self.adopt(*id);
                Ok(*id)
            }
            GraphEdit::Connect { from, to, port } => {
                let from = self.must(from)?;
                self.bind(to, port, Input::wire(from))
            }
            GraphEdit::SetInput { node, port, input } => self.bind(node, port, input.clone()),
            GraphEdit::SetLabel { node, label } => {
                let id = self.must(node)?;
                if let Some(label) = label {
                    self.check_label(label, Some(id))?;
                }
                if let Some(n) = self.node_mut(id) {
                    n.label.clone_from(label);
                }
                self.adopt(id);
                Ok(id)
            }
            GraphEdit::SetPos { node, pos } => {
                let id = self.must(node)?;
                if let Some(n) = self.node_mut(id) {
                    n.pos = *pos;
                }
                Ok(id)
            }
            GraphEdit::SetKind { node, kind } => {
                let id = self.must(node)?;
                if let Some(n) = self.node_mut(id) {
                    n.kind = kind.clone();
                    let ports = n.kind.input_ports().len();
                    n.inputs.truncate(ports);
                }
                Ok(id)
            }
            GraphEdit::Remove { node } => {
                let id = self.must(node)?;
                self.remove(id);
                Ok(id)
            }
        }
    }

    fn must(&self, node: &NodeRef) -> Result<StableId, EditError> {
        self.resolve(node)
            .ok_or_else(|| EditError::UnknownNode(node.clone()))
    }

    /// Refuses an empty or reserved label, or one another node (not `own`)
    /// holds.
    fn check_label(&self, label: &str, own: Option<StableId>) -> Result<(), EditError> {
        if label.trim().is_empty() || label == RESERVED_LABEL {
            return Err(EditError::BadLabel(label.to_owned()));
        }
        match self.by_label(label) {
            Some(other) if Some(other.id) != own => Err(EditError::LabelTaken(label.to_owned())),
            _ => Ok(()),
        }
    }

    /// Declares the node labelled `name`: updates its kind and bindings if
    /// one exists (keeping its id and position, so wires to it hold), or
    /// adds one with `id` below its column.
    fn declare(
        &mut self,
        id: StableId,
        name: &str,
        kind: NodeKind,
        inputs: impl FnOnce(&Self, StableId) -> Vec<Input>,
    ) -> Result<StableId, EditError> {
        self.check_label(name, self.by_label(name).map(|n| n.id))?;
        let id = match self.by_label(name) {
            Some(existing) => existing.id,
            None => {
                if self.node(id).is_some() {
                    return Err(EditError::IdTaken(id));
                }
                let pos = self.free_pos(&kind);
                self.insert(Node {
                    pos,
                    ..Node::new(id, kind.clone()).labelled(name)
                });
                id
            }
        };
        let bindings = inputs(self, id);
        if let Some(node) = self.node_mut(id) {
            node.kind = kind;
            node.inputs = bindings;
        }
        self.adopt(id);
        Ok(id)
    }

    /// Binds input `port` of `node`, filling any ports before it with their
    /// defaults (bindings are positional).
    fn bind(&mut self, node: &NodeRef, port: &PortRef, input: Input) -> Result<StableId, EditError> {
        let id = self.must(node)?;
        let Some(target) = self.node_mut(id) else {
            return Err(EditError::UnknownNode(node.clone()));
        };
        let index = port_index(&target.kind, port).ok_or_else(|| EditError::UnknownPort {
            node: node.clone(),
            port: port.clone(),
        })?;
        while target.inputs.len() <= index {
            let next = target.inputs.len();
            let Some(default) = target.input(next) else {
                break;
            };
            target.inputs.push(default);
        }
        if let Some(slot) = target.inputs.get_mut(index) {
            *slot = input;
        }
        Ok(id)
    }

    /// Turns every bus read of `id`'s label into a wire to it, so a node
    /// declared after its readers still wires to them (and a later rename
    /// keeps the wire).
    fn adopt(&mut self, id: StableId) {
        let Some(label) = self.node(id).and_then(|n| n.label.clone()) else {
            return;
        };
        for node in &mut self.nodes {
            for input in &mut node.inputs {
                if matches!(input, Input::External(read) if *read == label) {
                    *input = Input::wire(id);
                }
            }
        }
    }
}

/// The index of input `port` of a node of `kind`, if it has one.
pub fn port_index(kind: &NodeKind, port: &PortRef) -> Option<usize> {
    let count = kind.input_ports().len();
    match port {
        PortRef::Index(i) => (*i < count).then_some(*i),
        PortRef::Name(name) => {
            (0..count).find(|&i| kind.input_name(i).is_some_and(|n| n == name.as_str()))
        }
    }
}

/// The canvas column a new node of `kind` starts in: sources (params,
/// clocks, constants, oscillators, keys) left, everything else right of
/// them.
fn column_x(kind: &NodeKind) -> f32 {
    match kind {
        NodeKind::Param { .. }
        | NodeKind::Time
        | NodeKind::Constant
        | NodeKind::Oscillator
        | NodeKind::Key { .. } => 20.0,
        _ => 220.0,
    }
}
