//! The G-1 acceptance tests: today's signals convert and lower to equal
//! expressions, shape graphs lower to equal trees, and the rate rule holds.

use bevy::math::Vec2;
use gradiance_core::ids::StableId;
use gradiance_domain::shape::{CsgOp, MAX_CSG_DEPTH, ShapeDef};
use gradiance_domain::signal::{
    BlockOp, ComputedSignal, ComputedSignals, Curve, CurveInterp, SignalExpr, SignalParam,
    SignalParams,
};
use proptest::prelude::*;
use uuid::Uuid;

use crate::{
    CATALOG, Family, Graph, GraphError, Input, Node, NodeKind, PortType, Rate, Value, from_signals,
    lower_shape, lower_signal, lower_signals, meta,
};

fn id(n: u128) -> StableId {
    StableId(Uuid::from_u128(n))
}

/// Deterministic ids 1, 2, 3, ….
fn counter() -> impl FnMut() -> StableId {
    let mut n = 0;
    move || {
        n += 1;
        id(n)
    }
}

fn param(name: &str, value: f32) -> SignalParam {
    SignalParam {
        name: name.to_owned(),
        value,
        min: 0.0,
        max: 10.0,
    }
}

fn rpn(name: &str, source: &str) -> ComputedSignal {
    ComputedSignal {
        name: name.to_owned(),
        expr: SignalExpr::parse_rpn(source).unwrap(),
        block: None,
    }
}

fn bent_curve() -> Curve {
    Curve {
        points: vec![Vec2::ZERO, Vec2::new(0.3, 0.8), Vec2::ONE],
        interp: CurveInterp::Linear,
    }
}

/// Every `BlockOp` variant, each operand set to `operand`.
fn every_block(operand: Option<&str>) -> Vec<BlockOp> {
    let o = || operand.map(str::to_owned);
    vec![
        BlockOp::Time,
        BlockOp::Constant { value: 2.5 },
        BlockOp::Oscillator {
            amp: 3.0,
            freq: 0.5,
        },
        BlockOp::Gain { input: o(), k: 4.0 },
        BlockOp::Sum { a: o(), b: o() },
        BlockOp::Product { a: o(), b: o() },
        BlockOp::Sub { a: o(), b: o() },
        BlockOp::Abs { input: o() },
        BlockOp::Min { a: o(), b: o() },
        BlockOp::Max { a: o(), b: o() },
        BlockOp::Curve {
            input: o(),
            curve: bent_curve(),
        },
        BlockOp::Key {
            key: "e".to_owned(),
            negative: Some("arrowleft".to_owned()),
        },
    ]
}

/// Converts `params` + `computed` and checks every computed signal lowers
/// to exactly its `expr`; returns the graph.
fn assert_lowers_unchanged(params: Vec<SignalParam>, computed: Vec<ComputedSignal>) -> Graph {
    let params = SignalParams(params);
    let computed = ComputedSignals(computed);
    let graph = from_signals(&params, &computed, counter());
    for signal in &computed.0 {
        let node = graph.by_label(&signal.name).unwrap();
        assert_eq!(
            lower_signal(&graph, node.id).unwrap(),
            signal.expr,
            "{} lowers to its expression",
            signal.name
        );
    }
    graph
}

#[test]
fn every_block_converts_and_lowers_to_its_expression() {
    // Operands wired to a param, to another computed signal, to an
    // external name, and unconnected.
    for operand in [Some("p"), Some("clock"), Some("speed@x"), None] {
        for (i, block) in every_block(operand).into_iter().enumerate() {
            let graph = assert_lowers_unchanged(
                vec![param("p", 0.7)],
                vec![
                    ComputedSignal::from_block("clock", BlockOp::Time),
                    ComputedSignal::from_block(format!("out{i}"), block),
                ],
            );
            graph.validate().unwrap();
        }
    }
}

#[test]
fn operands_become_wires_by_id_or_external_reads() {
    let graph = from_signals(
        &SignalParams(vec![param("p", 1.0)]),
        &ComputedSignals(vec![ComputedSignal::from_block(
            "out",
            BlockOp::Sum {
                a: Some("p".into()),
                b: Some("t".into()),
            },
        )]),
        counter(),
    );
    let out = graph.by_label("out").unwrap();
    assert_eq!(
        out.inputs,
        vec![Input::wire(id(1)), Input::External("t".into())]
    );
    assert_eq!(out.kind, NodeKind::Sum);
}

#[test]
fn rpn_signals_convert_and_lower_to_their_expressions() {
    let graph = assert_lowers_unchanged(
        vec![param("amp", 2.0), param("x", 0.25)],
        vec![
            rpn("wave", "t sin amp *"),
            // A cycle between two computed signals.
            rpn("a", "b 1 +"),
            rpn("b", "a 2 *"),
            rpn("sensor", "speed@x abs 0.5 min"),
            rpn("square", "x x * y neg max"),
            rpn("mixed", "t 2 / cos osc +"),
            ComputedSignal::from_block(
                "osc",
                BlockOp::Oscillator {
                    amp: 1.0,
                    freq: 3.0,
                },
            ),
            rpn("decay", "decay 0.9 *"),
        ],
    );
    let order = graph.validate().unwrap();
    // a ↔ b and decay → decay are frame-rate cycles: legal, and broken at
    // the edges into their lowest-id node.
    let a = graph.by_label("a").unwrap().id;
    let b = graph.by_label("b").unwrap().id;
    let decay = graph.by_label("decay").unwrap().id;
    assert_eq!(order.feedback.len(), 2, "{order:?}");
    assert!(a.0 < b.0);
    assert!(order.feedback.iter().any(|f| f.to == a && f.from == b));
    assert!(
        order
            .feedback
            .iter()
            .any(|f| f.to == decay && f.from == decay)
    );
    assert_eq!(order.nodes.len(), graph.nodes.len());
}

#[test]
fn lower_signals_compiles_every_published_signal_but_params() {
    let graph = from_signals(
        &SignalParams(vec![param("k", 1.0)]),
        &ComputedSignals(vec![
            rpn("y", "x k *"),
            ComputedSignal::from_block("x", BlockOp::Time),
        ]),
        counter(),
    );
    let lowered = lower_signals(&graph).unwrap();
    let names: Vec<&str> = lowered.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["x", "y"], "sources first, params left out");
}

#[test]
fn an_unlabelled_node_is_inlined() {
    let gain = Node::new(id(1), NodeKind::Gain)
        .with_inputs(vec![Input::External("t".into()), Input::scalar(2.0)]);
    let sum = Node::new(id(2), NodeKind::Sum)
        .with_inputs(vec![Input::wire(id(1)), Input::scalar(1.0)])
        .labelled("out");
    let graph = Graph::from_nodes(vec![sum, gain]);
    let expected = SignalExpr::parse_rpn("t 2 * 1 +").unwrap();
    assert_eq!(lower_signal(&graph, id(2)).unwrap(), expected);
}

#[test]
fn a_cycle_of_unlabelled_nodes_is_refused() {
    let a = Node::new(id(1), NodeKind::Abs).with_inputs(vec![Input::wire(id(2))]);
    let b = Node::new(id(2), NodeKind::Abs).with_inputs(vec![Input::wire(id(1))]);
    let graph = Graph::from_nodes(vec![a, b]);
    assert_eq!(graph.validate(), Err(GraphError::UnlabelledCycle(id(1))));
}

fn box_node(n: u128, width: Input, height: f32) -> Node {
    Node::new(id(n), NodeKind::Box).with_inputs(vec![width, Input::scalar(height)])
}

fn shape_graph(nodes: Vec<Node>, root: u128) -> Result<ShapeDef, GraphError> {
    lower_shape(&Graph::from_nodes(nodes), id(root))
}

#[test]
fn every_shape_leaf_lowers_to_an_equal_tree() {
    let outline = vec![
        Vec2::new(-1.0, -1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(0.0, 1.0),
    ];
    let holes = vec![vec![
        Vec2::new(0.0, 0.2),
        Vec2::new(0.1, -0.2),
        Vec2::new(-0.1, -0.2),
    ]];
    let cases = [
        (
            box_node(1, Input::scalar(2.0), 3.0),
            ShapeDef::Box {
                width: 2.0,
                height: 3.0,
            },
        ),
        (
            Node::new(id(1), NodeKind::Circle).with_inputs(vec![Input::scalar(0.4)]),
            ShapeDef::Circle { radius: 0.4 },
        ),
        (
            Node::new(id(1), NodeKind::Polygon).with_inputs(vec![
                Input::Const(Value::Points(outline.clone())),
                Input::Const(Value::Contours(holes.clone())),
            ]),
            ShapeDef::Polygon { outline, holes },
        ),
        (
            Node::new(id(1), NodeKind::Capsule)
                .with_inputs(vec![Input::scalar(1.5), Input::scalar(0.2)]),
            ShapeDef::Capsule {
                half_length: 1.5,
                radius: 0.2,
            },
        ),
        (Node::new(id(1), NodeKind::HalfPlane), ShapeDef::HalfPlane),
        // Unconnected ports read their defaults.
        (Node::new(id(1), NodeKind::Box), ShapeDef::default()),
    ];
    for (node, expected) in cases {
        assert_eq!(shape_graph(vec![node], 1), Ok(expected));
    }
}

#[test]
fn every_csg_op_and_a_placement_lower_to_equal_trees() {
    let circle = Node::new(id(2), NodeKind::Circle).with_inputs(vec![Input::scalar(0.5)]);
    let unit = ShapeDef::Box {
        width: 1.0,
        height: 2.0,
    };
    for op in [
        CsgOp::Union,
        CsgOp::Subtract,
        CsgOp::Intersect,
        CsgOp::Xor,
        CsgOp::SmoothUnion { radius: 0.1 },
    ] {
        let csg = Node::new(id(3), NodeKind::Csg(op))
            .with_inputs(vec![Input::wire(id(1)), Input::wire(id(2))]);
        let got = shape_graph(
            vec![box_node(1, Input::scalar(1.0), 2.0), circle.clone(), csg],
            3,
        );
        assert_eq!(
            got,
            Ok(ShapeDef::Csg {
                op,
                lhs: Box::new(unit.clone()),
                rhs: Box::new(ShapeDef::Circle { radius: 0.5 }),
            })
        );
    }
    let placed = Node::new(id(3), NodeKind::Placed).with_inputs(vec![
        Input::wire(id(2)),
        Input::Const(Value::Vec2(Vec2::new(1.0, -2.0))),
        Input::scalar(0.75),
    ]);
    // A node used twice is copied: the tree has no sharing.
    let twice = Node::new(id(4), NodeKind::Csg(CsgOp::Union))
        .with_inputs(vec![Input::wire(id(3)), Input::wire(id(2))]);
    let placed_tree = ShapeDef::Placed {
        pos: Vec2::new(1.0, -2.0),
        rot: 0.75,
        shape: Box::new(ShapeDef::Circle { radius: 0.5 }),
    };
    let nodes = vec![circle, placed, twice];
    assert_eq!(shape_graph(nodes.clone(), 3), Ok(placed_tree.clone()));
    assert_eq!(
        shape_graph(nodes, 4),
        Ok(ShapeDef::Csg {
            op: CsgOp::Union,
            lhs: Box::new(placed_tree),
            rhs: Box::new(ShapeDef::Circle { radius: 0.5 }),
        })
    );
}

#[test]
fn a_frame_rate_output_into_a_shape_input_is_a_rate_mismatch() {
    let osc = Node::new(id(1), NodeKind::Oscillator);
    let width = box_node(2, Input::wire(id(1)), 1.0);
    assert_eq!(
        shape_graph(vec![osc, width.clone()], 2),
        Err(GraphError::RateMismatch {
            node: id(2),
            input: 0
        })
    );
    // An external bus read is frame rate too.
    let external = box_node(2, Input::External("t".into()), 1.0);
    assert!(matches!(
        shape_graph(vec![external], 2),
        Err(GraphError::RateMismatch { .. })
    ));
    // A param is an edit-rate scalar: it sizes the box, and the same param
    // may feed a signal too.
    let param = Node::new(
        id(1),
        NodeKind::Param {
            value: 2.5,
            min: 0.0,
            max: 5.0,
        },
    )
    .labelled("size");
    let gain = Node::new(id(3), NodeKind::Gain)
        .with_inputs(vec![Input::wire(id(1)), Input::scalar(2.0)])
        .labelled("doubled");
    let graph = Graph::from_nodes(vec![param, width, gain]);
    assert_eq!(
        lower_shape(&graph, id(2)),
        Ok(ShapeDef::Box {
            width: 2.5,
            height: 1.0
        })
    );
    assert_eq!(
        lower_signal(&graph, id(3)),
        Ok(SignalExpr::parse_rpn("size 2 *").unwrap())
    );
}

#[test]
fn a_shape_deeper_than_max_csg_depth_is_refused() {
    // A box under n placements is n + 1 deep.
    let chain = |placements: u128| {
        let mut nodes = vec![box_node(1, Input::scalar(1.0), 1.0)];
        for n in 2..=placements + 1 {
            nodes
                .push(Node::new(id(n), NodeKind::Placed).with_inputs(vec![Input::wire(id(n - 1))]));
        }
        Graph::from_nodes(nodes)
    };
    let max = u128::from(MAX_CSG_DEPTH);
    let deepest = chain(max - 1);
    assert_eq!(
        lower_shape(&deepest, id(max)).unwrap().depth(),
        MAX_CSG_DEPTH
    );
    assert_eq!(
        chain(max).validate(),
        Err(GraphError::TooDeep {
            node: id(max + 1),
            depth: MAX_CSG_DEPTH + 1
        })
    );
    // A deep constant counts too.
    let mut deep = ShapeDef::default();
    for _ in 1..MAX_CSG_DEPTH {
        deep = ShapeDef::Placed {
            pos: Vec2::ZERO,
            rot: 0.0,
            shape: Box::new(deep),
        };
    }
    let csg = Node::new(id(1), NodeKind::Csg(CsgOp::Union))
        .with_inputs(vec![Input::Const(Value::Shape(deep))]);
    assert!(matches!(
        Graph::from_nodes(vec![csg]).validate(),
        Err(GraphError::TooDeep { .. })
    ));
}

#[test]
fn validation_names_each_broken_binding() {
    let err = |nodes: Vec<Node>| Graph::from_nodes(nodes).validate().unwrap_err();
    assert_eq!(
        err(vec![box_node(1, Input::wire(id(9)), 1.0)]),
        GraphError::UnknownSource {
            node: id(1),
            input: 0,
            from: id(9)
        }
    );
    assert_eq!(
        err(vec![
            Node::new(id(1), NodeKind::Time),
            Node::new(id(2), NodeKind::Abs).with_inputs(vec![Input::Wire {
                node: id(1),
                port: 1
            }]),
        ]),
        GraphError::UnknownOutput {
            node: id(2),
            input: 0,
            port: 1
        }
    );
    assert_eq!(
        err(vec![
            Node::new(id(1), NodeKind::Abs)
                .with_inputs(vec![Input::scalar(1.0), Input::scalar(2.0)])
        ]),
        GraphError::UnknownInput {
            node: id(1),
            input: 1
        }
    );
    // A shape into a scalar port.
    assert_eq!(
        err(vec![
            Node::new(id(1), NodeKind::HalfPlane),
            Node::new(id(2), NodeKind::Abs).with_inputs(vec![Input::wire(id(1))]),
        ]),
        GraphError::TypeMismatch {
            node: id(2),
            input: 0,
            expected: PortType::Scalar,
            found: PortType::Shape
        }
    );
    assert_eq!(
        err(vec![box_node(1, Input::scalar(-1.0), 1.0)]),
        GraphError::OutOfRange {
            node: id(1),
            input: 0,
            value: -1.0
        }
    );
    assert_eq!(
        err(vec![
            Node::new(id(1), NodeKind::Time).labelled("x"),
            Node::new(id(2), NodeKind::Time).labelled("x"),
        ]),
        GraphError::DuplicateLabel("x".into())
    );
    // A shape that contains itself.
    assert_eq!(
        err(vec![
            Node::new(id(1), NodeKind::Csg(CsgOp::Union)).with_inputs(vec![Input::wire(id(1))]),
        ]),
        GraphError::EditCycle(id(1))
    );
    let mut unsorted = Graph::from_nodes(vec![Node::new(id(1), NodeKind::Time)]);
    unsorted.nodes.insert(0, Node::new(id(2), NodeKind::Time));
    assert_eq!(unsorted.validate(), Err(GraphError::Unsorted(id(1))));
    unsorted.nodes[0].id = id(1);
    assert_eq!(unsorted.validate(), Err(GraphError::DuplicateNode(id(1))));
}

#[test]
fn lowering_refuses_the_other_family() {
    let graph = Graph::from_nodes(vec![
        Node::new(id(1), NodeKind::Time),
        Node::new(id(2), NodeKind::HalfPlane),
    ]);
    assert_eq!(lower_shape(&graph, id(1)), Err(GraphError::NotShape(id(1))));
    assert_eq!(
        lower_signal(&graph, id(2)),
        Err(GraphError::NotSignal(id(2)))
    );
    assert_eq!(
        lower_signal(&graph, id(3)),
        Err(GraphError::UnknownNode(id(3)))
    );
    // A degenerate tree fails `ShapeDef::validate`.
    let flat = Node::new(id(1), NodeKind::Polygon)
        .with_inputs(vec![Input::Const(Value::Points(vec![Vec2::ZERO]))]);
    assert!(matches!(
        shape_graph(vec![flat], 1),
        Err(GraphError::Shape { .. })
    ));
}

/// One node of every kind (the match makes a new kind a compile error
/// here until it is listed).
fn every_kind() -> Vec<NodeKind> {
    let kinds = vec![
        NodeKind::Time,
        NodeKind::Constant,
        NodeKind::Oscillator,
        NodeKind::Gain,
        NodeKind::Sum,
        NodeKind::Product,
        NodeKind::Sub,
        NodeKind::Abs,
        NodeKind::Min,
        NodeKind::Max,
        NodeKind::Curve,
        NodeKind::Key {
            key: "e".into(),
            negative: None,
        },
        NodeKind::Pad {
            input: "LeftStickX".into(),
            dead_zone: 0.1,
        },
        NodeKind::Param {
            value: 1.0,
            min: 0.0,
            max: 2.0,
        },
        NodeKind::Expr(SignalExpr::parse_rpn("u v + u *").unwrap()),
        NodeKind::Box,
        NodeKind::Circle,
        NodeKind::Polygon,
        NodeKind::Capsule,
        NodeKind::HalfPlane,
        NodeKind::Csg(CsgOp::Xor),
        NodeKind::Placed,
    ];
    for kind in &kinds {
        match kind {
            NodeKind::Time
            | NodeKind::Constant
            | NodeKind::Oscillator
            | NodeKind::Gain
            | NodeKind::Sum
            | NodeKind::Product
            | NodeKind::Sub
            | NodeKind::Abs
            | NodeKind::Min
            | NodeKind::Max
            | NodeKind::Curve
            | NodeKind::Key { .. }
            | NodeKind::Pad { .. }
            | NodeKind::Param { .. }
            | NodeKind::Expr(_)
            | NodeKind::Box
            | NodeKind::Circle
            | NodeKind::Polygon
            | NodeKind::Capsule
            | NodeKind::HalfPlane
            | NodeKind::Csg(_)
            | NodeKind::Placed => {}
        }
    }
    kinds
}

#[test]
fn every_kind_has_one_catalog_row() {
    let kinds = every_kind();
    let mut ids: Vec<&str> = kinds.iter().map(|k| meta(k).id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), kinds.len(), "one row per kind");
    assert_eq!(CATALOG.len(), kinds.len(), "no row without a kind");
    for kind in &kinds {
        let row = meta(kind);
        let family = if row.id.starts_with("signal::") {
            Family::Signal
        } else {
            Family::Shape
        };
        assert_eq!(row.family, family, "{}", row.id);
        let rate = match (family, kind) {
            (Family::Signal, NodeKind::Param { .. }) | (Family::Shape, _) => Rate::Edit,
            (Family::Signal, _) => Rate::Frame,
        };
        assert_eq!(row.rate, rate, "{}", row.id);
        assert_eq!(row.outputs.len(), 1, "{}", row.id);
        assert!(!row.category.is_empty() && !row.name.is_empty() && !row.doc.is_empty());
        for port in kind.input_ports().iter() {
            assert_eq!(
                port.default.value().port_type(),
                port.ty,
                "{}.{}",
                row.id,
                port.name
            );
        }
    }
}

#[test]
fn lowering_reads_every_catalog_port() {
    for kind in every_kind() {
        let ports = kind.input_ports();
        let row = meta(&kind);
        match row.family {
            Family::Signal => {
                // Each scalar port wired to its own external name shows up
                // in the lowered expression.
                let inputs: Vec<Input> = ports
                    .iter()
                    .enumerate()
                    .map(|(i, p)| match p.ty {
                        PortType::Scalar => Input::External(format!("port{i}")),
                        _ => Input::Const(p.default.value()),
                    })
                    .collect();
                let node = Node::new(id(1), kind.clone()).with_inputs(inputs);
                let expr = lower_signal(&Graph::from_nodes(vec![node]), id(1)).unwrap();
                let read = expr.inputs();
                for (i, p) in ports.iter().enumerate() {
                    if p.ty == PortType::Scalar {
                        assert!(
                            read.contains(&format!("port{i}")),
                            "{} reads {}",
                            row.id,
                            p.name
                        );
                    }
                }
            }
            Family::Shape => {
                // Changing each scalar port's constant changes the tree.
                let base = Node::new(id(1), kind.clone());
                let default = shape_graph(vec![base.clone()], 1).unwrap();
                for (i, p) in ports.iter().enumerate() {
                    if p.ty != PortType::Scalar {
                        continue;
                    }
                    let mut inputs: Vec<Input> = ports
                        .iter()
                        .map(|p| Input::Const(p.default.value()))
                        .collect();
                    inputs[i] = Input::scalar(0.123);
                    let changed = shape_graph(vec![base.clone().with_inputs(inputs)], 1).unwrap();
                    assert_ne!(changed, default, "{} reads {}", row.id, p.name);
                }
            }
        }
    }
}

#[test]
fn a_graph_round_trips_through_ron() {
    let mut nodes: Vec<Node> = every_kind()
        .into_iter()
        .enumerate()
        .map(|(i, kind)| {
            let mut node = Node::new(id(i as u128 + 1), kind);
            node.pos = Vec2::new(i as f32 * 1.5, -0.25);
            node
        })
        .collect();
    nodes[3].inputs = vec![Input::wire(id(13)), Input::External("t".into())];
    nodes[3].label = Some("gain".into());
    nodes[10].inputs = vec![Input::scalar(0.5), Input::Const(Value::Curve(bent_curve()))];
    nodes[19].inputs = vec![
        Input::Const(Value::Shape(ShapeDef::Circle { radius: 0.3 })),
        Input::wire(id(15)),
    ];
    let graph = Graph::from_nodes(nodes);
    graph.validate().unwrap();
    let text = ron::ser::to_string_pretty(&graph, ron::ser::PrettyConfig::default()).unwrap();
    let back: Graph = ron::from_str(&text).unwrap();
    assert_eq!(back, graph);
}

/// A random expression over the names `a`, `b`, `p`, `t` and `ext`.
fn expr_strategy() -> impl Strategy<Value = SignalExpr> {
    let leaf = prop_oneof![
        (-4.0f32..4.0).prop_map(SignalExpr::Const),
        prop::sample::select(vec!["a", "b", "p", "t", "ext"])
            .prop_map(|n| SignalExpr::Input(n.to_owned())),
    ];
    leaf.prop_recursive(4, 24, 2, |inner| {
        let b = |e| Box::new(e);
        prop_oneof![
            inner.clone().prop_map(move |e| SignalExpr::Neg(b(e))),
            inner.clone().prop_map(move |e| SignalExpr::Sin(b(e))),
            inner.clone().prop_map(move |e| SignalExpr::Abs(b(e))),
            (inner.clone(), inner.clone()).prop_map(move |(x, y)| SignalExpr::Add(b(x), b(y))),
            (inner.clone(), inner.clone()).prop_map(move |(x, y)| SignalExpr::Div(b(x), b(y))),
            (inner.clone(), inner.clone()).prop_map(move |(x, y)| SignalExpr::Max(b(x), b(y))),
            (inner.clone(), bent_curve_strategy())
                .prop_map(move |(x, c)| SignalExpr::Curve(b(x), c)),
        ]
    })
}

fn bent_curve_strategy() -> impl Strategy<Value = Curve> {
    Just(bent_curve())
}

proptest! {
    /// Any pair of computed expressions (cycles included) over a param,
    /// each other, the clock and an external name lowers back unchanged.
    #[test]
    fn any_expression_lowers_unchanged(a in expr_strategy(), b in expr_strategy()) {
        let computed = vec![
            ComputedSignal { name: "a".into(), expr: a, block: None },
            ComputedSignal { name: "b".into(), expr: b, block: None },
        ];
        let graph = assert_lowers_unchanged(vec![param("p", 0.5)], computed);
        prop_assert!(graph.validate().is_ok());
    }
}
