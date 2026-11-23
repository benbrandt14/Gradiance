//! The converter from today's by-name signal resources to a graph.

use std::collections::BTreeMap;

use gradiance_core::ids::StableId;
use gradiance_domain::signal::{BlockOp, ComputedSignals, SignalParams};

use crate::ir::{Graph, Input, Node, NodeKind, Value};

/// Converts `params` and `computed` to one graph: a
/// [`Param`](NodeKind::Param) node per param, a block node per canvas
/// block, an [`Expr`](NodeKind::Expr) node per console `defsignal`, each
/// labelled with its bus name and placed at the origin (positions are not
/// saved before G-2).
///
/// Every operand name that a param or a computed signal publishes becomes
/// a wire to that node (the first producer wins; a duplicate name is a
/// [`DuplicateLabel`](crate::GraphError::DuplicateLabel) at validation);
/// every other name becomes an [`External`](Input::External) read. An
/// unconnected block operand is a constant `0`, as `BlockOp::to_expr`
/// reads it. `ids` supplies each new node's id, params first, then
/// computed signals, in order (so a test can make them deterministic).
pub fn from_signals(
    params: &SignalParams,
    computed: &ComputedSignals,
    mut ids: impl FnMut() -> StableId,
) -> Graph {
    let param_ids: Vec<StableId> = params.0.iter().map(|_| ids()).collect();
    let computed_ids: Vec<StableId> = computed.0.iter().map(|_| ids()).collect();

    let mut producers: BTreeMap<&str, StableId> = BTreeMap::new();
    let names = params
        .0
        .iter()
        .map(|p| p.name.as_str())
        .zip(param_ids.iter())
        .chain(
            computed
                .0
                .iter()
                .map(|c| c.name.as_str())
                .zip(computed_ids.iter()),
        );
    for (name, id) in names {
        producers.entry(name).or_insert(*id);
    }
    let resolve = |name: &str| match producers.get(name) {
        Some(id) => Input::wire(*id),
        None => Input::External(name.to_owned()),
    };
    let operand = |name: &Option<String>| match name {
        Some(name) => resolve(name),
        None => Input::scalar(0.0),
    };

    let mut nodes = Vec::with_capacity(param_ids.len() + computed_ids.len());
    for (param, id) in params.0.iter().zip(param_ids) {
        let kind = NodeKind::Param {
            value: param.value,
            min: param.min,
            max: param.max,
        };
        nodes.push(Node::new(id, kind).labelled(param.name.clone()));
    }
    for (signal, id) in computed.0.iter().zip(computed_ids) {
        let (kind, inputs) = match &signal.block {
            Some(block) => block_node(block, &operand),
            None => (
                NodeKind::Expr(signal.expr.clone()),
                signal.expr.inputs().iter().map(|n| resolve(n)).collect(),
            ),
        };
        nodes.push(
            Node::new(id, kind)
                .with_inputs(inputs)
                .labelled(signal.name.clone()),
        );
    }
    Graph::from_nodes(nodes)
}

/// The node kind and bindings of a canvas block.
fn block_node(
    block: &BlockOp,
    operand: &impl Fn(&Option<String>) -> Input,
) -> (NodeKind, Vec<Input>) {
    let c = Input::scalar;
    match block {
        BlockOp::Time => (NodeKind::Time, Vec::new()),
        BlockOp::Constant { value } => (NodeKind::Constant, vec![c(*value)]),
        BlockOp::Oscillator { amp, freq } => (NodeKind::Oscillator, vec![c(*amp), c(*freq)]),
        BlockOp::Gain { input, k } => (NodeKind::Gain, vec![operand(input), c(*k)]),
        BlockOp::Sum { a, b } => (NodeKind::Sum, vec![operand(a), operand(b)]),
        BlockOp::Product { a, b } => (NodeKind::Product, vec![operand(a), operand(b)]),
        BlockOp::Sub { a, b } => (NodeKind::Sub, vec![operand(a), operand(b)]),
        BlockOp::Abs { input } => (NodeKind::Abs, vec![operand(input)]),
        BlockOp::Min { a, b } => (NodeKind::Min, vec![operand(a), operand(b)]),
        BlockOp::Max { a, b } => (NodeKind::Max, vec![operand(a), operand(b)]),
        BlockOp::Curve { input, curve } => (
            NodeKind::Curve,
            vec![operand(input), Input::Const(Value::Curve(curve.clone()))],
        ),
        BlockOp::Key { key, negative } => (
            NodeKind::Key {
                key: key.clone(),
                negative: negative.clone(),
            },
            Vec::new(),
        ),
        BlockOp::Pad { input, dead_zone } => (
            NodeKind::Pad {
                input: input.clone(),
                dead_zone: *dead_zone,
            },
            Vec::new(),
        ),
    }
}
