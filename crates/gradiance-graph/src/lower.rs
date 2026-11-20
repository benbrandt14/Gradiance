//! The two lowerings: the signal family to one [`SignalExpr`] per
//! published node, the shape family to a [`ShapeDef`].

use gradiance_core::ids::StableId;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::signal::{SignalExpr, key_input_name};

use crate::ir::{Family, Graph, Input, Node, NodeKind, PortType, Value};
use crate::validate::GraphError;

/// The bus input of the elapsed-seconds clock (the signal runtime's
/// `TIME_INPUT`; this crate sits below it).
const TIME_INPUT: &str = "t";

/// Lowers signal node `id` to the expression the signal runtime compiles.
///
/// Validates the whole graph first. A wire to a labelled node reads the
/// bus under its label (`Input(label)`), so a cycle reads last frame's
/// value as today; a wire to an unlabelled node inlines its expression;
/// `External(name)` reads `Input(name)`; a constant is `Const`. A
/// [`Param`](NodeKind::Param) lowers to its value.
pub fn lower_signal(graph: &Graph, id: StableId) -> Result<SignalExpr, GraphError> {
    graph.validate()?;
    let node = graph.node(id).ok_or(GraphError::UnknownNode(id))?;
    if node.kind.family() != Family::Signal {
        return Err(GraphError::NotSignal(id));
    }
    signal_expr(graph, node)
}

/// Lowers every labelled signal node except params (a param publishes its
/// value directly, as today), in evaluation order: one `(label, expr)`
/// per compiled bus value. Validates once.
pub fn lower_signals(graph: &Graph) -> Result<Vec<(String, SignalExpr)>, GraphError> {
    let order = graph.validate()?;
    let mut out = Vec::new();
    for id in order.nodes {
        let Some(node) = graph.node(id) else { continue };
        let Some(label) = &node.label else { continue };
        if node.kind.family() != Family::Signal || matches!(node.kind, NodeKind::Param { .. }) {
            continue;
        }
        out.push((label.clone(), signal_expr(graph, node)?));
    }
    Ok(out)
}

/// Lowers the shape subgraph rooted at shape node `id` to a tree, and runs
/// `ShapeDef::validate` on it. A node used twice is copied (the tree has
/// no sharing). Validates the whole graph first.
pub fn lower_shape(graph: &Graph, id: StableId) -> Result<ShapeDef, GraphError> {
    graph.validate()?;
    let node = graph.node(id).ok_or(GraphError::UnknownNode(id))?;
    if node.kind.family() != Family::Shape {
        return Err(GraphError::NotShape(id));
    }
    let shape = shape_of(graph, node)?;
    shape
        .validate()
        .map_err(|error| GraphError::Shape { node: id, error })?;
    Ok(shape)
}

fn mismatch(node: &Node, input: usize, expected: PortType, found: PortType) -> GraphError {
    GraphError::TypeMismatch {
        node: node.id,
        input,
        expected,
        found,
    }
}

/// The binding of port `i`, or the error validation would have given.
fn binding(node: &Node, i: usize) -> Result<Input, GraphError> {
    node.input(i).ok_or(GraphError::UnknownInput {
        node: node.id,
        input: i,
    })
}

fn source<'g>(
    graph: &'g Graph,
    node: &Node,
    input: usize,
    id: StableId,
) -> Result<&'g Node, GraphError> {
    graph.node(id).ok_or(GraphError::UnknownSource {
        node: node.id,
        input,
        from: id,
    })
}

fn boxed(expr: SignalExpr) -> Box<SignalExpr> {
    Box::new(expr)
}

/// Scalar input `i` of a signal node as an expression.
fn scalar_expr(graph: &Graph, node: &Node, i: usize) -> Result<SignalExpr, GraphError> {
    match binding(node, i)? {
        Input::Const(Value::Scalar(c)) => Ok(SignalExpr::Const(c)),
        Input::Const(other) => Err(mismatch(node, i, PortType::Scalar, other.port_type())),
        Input::External(name) => Ok(SignalExpr::Input(name)),
        Input::Wire { node: id, .. } => {
            let src = source(graph, node, i, id)?;
            match &src.label {
                Some(label) => Ok(SignalExpr::Input(label.clone())),
                None => signal_expr(graph, src),
            }
        }
    }
}

/// A signal node's own expression over its lowered inputs. Each arm is the
/// expression `BlockOp::to_expr` builds for the same block.
fn signal_expr(graph: &Graph, node: &Node) -> Result<SignalExpr, GraphError> {
    let x = |i| scalar_expr(graph, node, i);
    let clock = || SignalExpr::Input(TIME_INPUT.to_owned());
    Ok(match &node.kind {
        NodeKind::Time => clock(),
        NodeKind::Constant => x(0)?,
        NodeKind::Oscillator => SignalExpr::Mul(
            boxed(x(0)?),
            boxed(SignalExpr::Sin(boxed(SignalExpr::Mul(
                boxed(x(1)?),
                boxed(clock()),
            )))),
        ),
        // `in · k` and `a · b` are one product.
        NodeKind::Gain | NodeKind::Product => SignalExpr::Mul(boxed(x(0)?), boxed(x(1)?)),
        NodeKind::Sum => SignalExpr::Add(boxed(x(0)?), boxed(x(1)?)),
        NodeKind::Sub => SignalExpr::Sub(boxed(x(0)?), boxed(x(1)?)),
        NodeKind::Abs => SignalExpr::Abs(boxed(x(0)?)),
        NodeKind::Min => SignalExpr::Min(boxed(x(0)?), boxed(x(1)?)),
        NodeKind::Max => SignalExpr::Max(boxed(x(0)?), boxed(x(1)?)),
        NodeKind::Curve => {
            let curve = match binding(node, 1)? {
                Input::Const(Value::Curve(c)) => c,
                Input::Const(other) => {
                    return Err(mismatch(node, 1, PortType::Curve, other.port_type()));
                }
                // No node outputs a curve yet, and the bus carries scalars.
                Input::Wire { .. } | Input::External(_) => {
                    return Err(mismatch(node, 1, PortType::Curve, PortType::Scalar));
                }
            };
            SignalExpr::Curve(boxed(x(0)?), curve)
        }
        NodeKind::Key { key, negative } => {
            let held = |name: &str| SignalExpr::Input(key_input_name(name));
            match negative {
                Some(negative) => SignalExpr::Sub(boxed(held(key)), boxed(held(negative))),
                None => held(key),
            }
        }
        NodeKind::Param { value, .. } => SignalExpr::Const(*value),
        NodeKind::Expr(expr) => {
            let names = expr.inputs();
            let args = (0..names.len()).map(x).collect::<Result<Vec<_>, _>>()?;
            substitute(expr, &names, &args)
        }
        NodeKind::Box
        | NodeKind::Circle
        | NodeKind::Polygon
        | NodeKind::Capsule
        | NodeKind::HalfPlane
        | NodeKind::Csg(_)
        | NodeKind::Placed => return Err(GraphError::NotSignal(node.id)),
    })
}

/// `expr` with every read of `names[i]` replaced by `args[i]`.
fn substitute(expr: &SignalExpr, names: &[String], args: &[SignalExpr]) -> SignalExpr {
    let s = |e: &SignalExpr| boxed(substitute(e, names, args));
    match expr {
        SignalExpr::Const(c) => SignalExpr::Const(*c),
        SignalExpr::Input(name) => names
            .iter()
            .position(|n| n == name)
            .and_then(|i| args.get(i))
            .cloned()
            .unwrap_or_else(|| SignalExpr::Input(name.clone())),
        SignalExpr::Neg(a) => SignalExpr::Neg(s(a)),
        SignalExpr::Sin(a) => SignalExpr::Sin(s(a)),
        SignalExpr::Cos(a) => SignalExpr::Cos(s(a)),
        SignalExpr::Abs(a) => SignalExpr::Abs(s(a)),
        SignalExpr::Add(a, b) => SignalExpr::Add(s(a), s(b)),
        SignalExpr::Sub(a, b) => SignalExpr::Sub(s(a), s(b)),
        SignalExpr::Mul(a, b) => SignalExpr::Mul(s(a), s(b)),
        SignalExpr::Div(a, b) => SignalExpr::Div(s(a), s(b)),
        SignalExpr::Min(a, b) => SignalExpr::Min(s(a), s(b)),
        SignalExpr::Max(a, b) => SignalExpr::Max(s(a), s(b)),
        SignalExpr::Curve(a, curve) => SignalExpr::Curve(s(a), curve.clone()),
    }
}

/// Edit-rate scalar input `i` of a shape node: a constant or a param's
/// value.
fn edit_scalar(graph: &Graph, node: &Node, i: usize) -> Result<f32, GraphError> {
    match binding(node, i)? {
        Input::Const(Value::Scalar(c)) => Ok(c),
        Input::Const(other) => Err(mismatch(node, i, PortType::Scalar, other.port_type())),
        Input::External(_) => Err(GraphError::RateMismatch {
            node: node.id,
            input: i,
        }),
        Input::Wire { node: id, .. } => match &source(graph, node, i, id)?.kind {
            NodeKind::Param { value, .. } => Ok(*value),
            _ => Err(GraphError::RateMismatch {
                node: node.id,
                input: i,
            }),
        },
    }
}

/// A constant-only input of a shape node (no node outputs these types yet).
fn edit_const(node: &Node, i: usize, ty: PortType) -> Result<Value, GraphError> {
    match binding(node, i)? {
        Input::Const(v) if v.port_type() == ty => Ok(v),
        Input::Const(other) => Err(mismatch(node, i, ty, other.port_type())),
        Input::Wire { .. } | Input::External(_) => Err(mismatch(node, i, ty, PortType::Scalar)),
    }
}

fn shape_input(graph: &Graph, node: &Node, i: usize) -> Result<Box<ShapeDef>, GraphError> {
    match binding(node, i)? {
        Input::Const(Value::Shape(s)) => Ok(Box::new(s)),
        Input::Const(other) => Err(mismatch(node, i, PortType::Shape, other.port_type())),
        Input::External(_) => Err(mismatch(node, i, PortType::Shape, PortType::Scalar)),
        Input::Wire { node: id, .. } => Ok(Box::new(shape_of(graph, source(graph, node, i, id)?)?)),
    }
}

fn shape_of(graph: &Graph, node: &Node) -> Result<ShapeDef, GraphError> {
    let x = |i| edit_scalar(graph, node, i);
    Ok(match &node.kind {
        NodeKind::Box => ShapeDef::Box {
            width: x(0)?,
            height: x(1)?,
        },
        NodeKind::Circle => ShapeDef::Circle { radius: x(0)? },
        NodeKind::Polygon => {
            let Value::Points(outline) = edit_const(node, 0, PortType::Points)? else {
                return Err(GraphError::NotShape(node.id));
            };
            let Value::Contours(holes) = edit_const(node, 1, PortType::Contours)? else {
                return Err(GraphError::NotShape(node.id));
            };
            ShapeDef::Polygon { outline, holes }
        }
        NodeKind::Capsule => ShapeDef::Capsule {
            half_length: x(0)?,
            radius: x(1)?,
        },
        NodeKind::HalfPlane => ShapeDef::HalfPlane,
        NodeKind::Csg(op) => ShapeDef::Csg {
            op: *op,
            lhs: shape_input(graph, node, 0)?,
            rhs: shape_input(graph, node, 1)?,
        },
        NodeKind::Placed => {
            let Value::Vec2(pos) = edit_const(node, 1, PortType::Vec2)? else {
                return Err(GraphError::NotShape(node.id));
            };
            ShapeDef::Placed {
                pos,
                rot: x(2)?,
                shape: shape_input(graph, node, 0)?,
            }
        }
        _ => return Err(GraphError::NotShape(node.id)),
    })
}
