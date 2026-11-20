//! The node catalog: one [`NodeMeta`] per [`NodeKind`], in Graphite's
//! vocabulary (`id`, `category`, `name`, doc; ports with `default` and
//! `hard` range — `docs/unified-surface-plan.md` § 5–6). A generated
//! `docs/nodes.md` projects [`CATALOG`] from G-2 on.

use bevy::math::Vec2;
use gradiance_domain::shape::{MIN_SHAPE_SIZE, ShapeDef};
use gradiance_domain::signal::Curve;

use crate::ir::{Family, NodeKind, PortType, Rate, Value};

/// What an unconnected port reads. A `const`-friendly description of a
/// [`Value`] (a `Value` may own a `Vec`, so it cannot sit in a static row).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortDefault {
    /// A scalar.
    Scalar(f32),
    /// A point.
    Vec2(Vec2),
    /// A point list.
    Points(&'static [Vec2]),
    /// No contours.
    NoContours,
    /// The default shape, a 1 m box (`ShapeDef::default`).
    UnitBox,
    /// The identity curve (`Curve::default`).
    IdentityCurve,
}

impl PortDefault {
    /// The constant this default stands for.
    pub fn value(&self) -> Value {
        match self {
            Self::Scalar(v) => Value::Scalar(*v),
            Self::Vec2(v) => Value::Vec2(*v),
            Self::Points(p) => Value::Points(p.to_vec()),
            Self::NoContours => Value::Contours(Vec::new()),
            Self::UnitBox => Value::Shape(ShapeDef::default()),
            Self::IdentityCurve => Value::Curve(Curve::default()),
        }
    }
}

/// One port of a catalog row.
#[derive(Debug, Clone, PartialEq)]
pub struct PortMeta {
    /// Port name (Graphite's parameter name). An
    /// [`Expr`](NodeKind::Expr) port is named by its expression
    /// ([`NodeKind::input_name`]).
    pub name: &'static str,
    /// Port type; a constant or a wire into it must have this type.
    pub ty: PortType,
    /// What it reads unconnected (Graphite's `default`).
    pub default: PortDefault,
    /// Inclusive range a scalar constant must lie in (Graphite's `hard`
    /// min/max); `None` for any value.
    pub hard: Option<(f32, f32)>,
}

impl PortMeta {
    const fn new(name: &'static str, ty: PortType, default: PortDefault) -> Self {
        Self {
            name,
            ty,
            default,
            hard: None,
        }
    }

    const fn scalar(name: &'static str, default: f32) -> Self {
        Self::new(name, PortType::Scalar, PortDefault::Scalar(default))
    }

    /// A length in metres: at least [`MIN_SHAPE_SIZE`].
    const fn length(name: &'static str, default: f32) -> Self {
        Self {
            name,
            ty: PortType::Scalar,
            default: PortDefault::Scalar(default),
            hard: Some((MIN_SHAPE_SIZE, f32::MAX)),
        }
    }

    /// The port of every [`Expr`](NodeKind::Expr) input: a scalar that
    /// reads 0 unconnected.
    pub(crate) const EXPR_INPUT: Self = Self::scalar("input", 0.0);
}

/// One catalog row: everything a view needs to show a node kind.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeMeta {
    /// Stable id (`family::kind`), the key a view and `docs/nodes.md` use.
    /// `::`, not `.`, so a node id never reads as an operation id
    /// (`signal.param` is the operation that edits a param;
    /// `signal::param` is the node).
    pub id: &'static str,
    /// Palette category.
    pub category: &'static str,
    /// Display name.
    pub name: &'static str,
    /// One-paragraph description.
    pub doc: &'static str,
    /// Which lowering the kind belongs to.
    pub family: Family,
    /// How often its output may change.
    pub rate: Rate,
    /// Input ports, in binding order. Empty for
    /// [`Expr`](NodeKind::Expr): its ports are its expression's names
    /// ([`NodeKind::input_ports`]).
    pub inputs: &'static [PortMeta],
    /// Output ports; an edge names one by index.
    pub outputs: &'static [PortMeta],
}

const SCALAR_OUT: &[PortMeta] = &[PortMeta::scalar("out", 0.0)];
const SHAPE_OUT: &[PortMeta] = &[PortMeta::new(
    "shape",
    PortType::Shape,
    PortDefault::UnitBox,
)];
const OPERAND: &[PortMeta] = &[PortMeta::scalar("in", 0.0)];
const OPERANDS: &[PortMeta] = &[PortMeta::scalar("a", 0.0), PortMeta::scalar("b", 0.0)];

/// The default polygon outline: a 1 m square, centroid-relative,
/// counter-clockwise.
const UNIT_SQUARE: &[Vec2] = &[
    Vec2::new(-0.5, -0.5),
    Vec2::new(0.5, -0.5),
    Vec2::new(0.5, 0.5),
    Vec2::new(-0.5, 0.5),
];

const fn signal(
    id: &'static str,
    category: &'static str,
    name: &'static str,
    doc: &'static str,
    inputs: &'static [PortMeta],
) -> NodeMeta {
    NodeMeta {
        id,
        category,
        name,
        doc,
        family: Family::Signal,
        rate: Rate::Frame,
        inputs,
        outputs: SCALAR_OUT,
    }
}

const fn shape(
    id: &'static str,
    category: &'static str,
    name: &'static str,
    doc: &'static str,
    inputs: &'static [PortMeta],
) -> NodeMeta {
    NodeMeta {
        id,
        category,
        name,
        doc,
        family: Family::Shape,
        rate: Rate::Edit,
        inputs,
        outputs: SHAPE_OUT,
    }
}

const SOURCES: &str = "Signal: Sources";
const MATH: &str = "Signal: Math";
const PRIMITIVES: &str = "Shape: Primitives";
const COMBINE: &str = "Shape: Combine";

/// Every catalog row, in palette order. [`meta`] maps each [`NodeKind`]
/// to its row; a test asserts the two agree.
pub static CATALOG: &[NodeMeta] = &[
    signal(
        "signal::time",
        SOURCES,
        "Time",
        "The elapsed-seconds clock t (the external bus input \"t\").",
        &[],
    ),
    signal(
        "signal::constant",
        SOURCES,
        "Constant",
        "A fixed value.",
        &[PortMeta::scalar("value", 0.0)],
    ),
    signal(
        "signal::oscillator",
        SOURCES,
        "Oscillator",
        "amp * sin(freq * t), freq in radians per second.",
        &[PortMeta::scalar("amp", 1.0), PortMeta::scalar("freq", 1.0)],
    ),
    signal(
        "signal::key",
        SOURCES,
        "Key",
        "1 while the key is held in play; with a negative key, +1, -1 or 0. \
         Reads the key@<name> bus inputs.",
        &[],
    ),
    NodeMeta {
        id: "signal::param",
        category: SOURCES,
        name: "Param",
        doc: "A tunable parameter (defparam): an edit-rate scalar that also \
              publishes each frame under its label, so it may feed both \
              families.",
        family: Family::Signal,
        rate: Rate::Edit,
        inputs: &[],
        outputs: SCALAR_OUT,
    },
    signal(
        "signal::gain",
        MATH,
        "Gain",
        "in * k.",
        &[PortMeta::scalar("in", 0.0), PortMeta::scalar("k", 1.0)],
    ),
    signal("signal::sum", MATH, "Sum", "a + b.", OPERANDS),
    signal("signal::product", MATH, "Product", "a * b.", OPERANDS),
    signal("signal::sub", MATH, "Sub", "a - b.", OPERANDS),
    signal("signal::abs", MATH, "Abs", "|in|.", OPERAND),
    signal(
        "signal::min",
        MATH,
        "Min",
        "The lesser of a and b.",
        OPERANDS,
    ),
    signal(
        "signal::max",
        MATH,
        "Max",
        "The greater of a and b.",
        OPERANDS,
    ),
    signal(
        "signal::curve",
        MATH,
        "Curve",
        "Reshapes in through a response curve over [0, 1].",
        &[
            PortMeta::scalar("in", 0.0),
            PortMeta::new("curve", PortType::Curve, PortDefault::IdentityCurve),
        ],
    ),
    signal(
        "signal::expr",
        MATH,
        "Expression",
        "A console defsignal expression; one scalar port per name it reads, \
         in first-appearance order.",
        &[],
    ),
    shape(
        "shape::box",
        PRIMITIVES,
        "Box",
        "An axis-aligned rectangle, full width and height in metres.",
        &[
            PortMeta::length("width", 1.0),
            PortMeta::length("height", 1.0),
        ],
    ),
    shape(
        "shape::circle",
        PRIMITIVES,
        "Circle",
        "A circle of radius metres.",
        &[PortMeta::length("radius", 0.5)],
    ),
    shape(
        "shape::polygon",
        PRIMITIVES,
        "Polygon",
        "A simple polygon: a counter-clockwise outline and clockwise holes.",
        &[
            PortMeta::new(
                "outline",
                PortType::Points,
                PortDefault::Points(UNIT_SQUARE),
            ),
            PortMeta::new("holes", PortType::Contours, PortDefault::NoContours),
        ],
    ),
    shape(
        "shape::capsule",
        PRIMITIVES,
        "Capsule",
        "The segment from -half_length to +half_length on x, inflated by \
         radius.",
        &[
            PortMeta::length("half_length", 0.5),
            PortMeta::length("radius", 0.1),
        ],
    ),
    shape(
        "shape::half_plane",
        PRIMITIVES,
        "Half-plane",
        "The infinite ground: solid below local y = 0.",
        &[],
    ),
    shape(
        "shape::csg",
        COMBINE,
        "Boolean",
        "A CSG boolean (union, subtract, intersect, xor, smooth union) of \
         lhs and rhs.",
        &[
            PortMeta::new("lhs", PortType::Shape, PortDefault::UnitBox),
            PortMeta::new("rhs", PortType::Shape, PortDefault::UnitBox),
        ],
    ),
    shape(
        "shape::placed",
        COMBINE,
        "Place",
        "A subtree moved to pos and rotated by rot radians (an isometry).",
        &[
            PortMeta::new("shape", PortType::Shape, PortDefault::UnitBox),
            PortMeta::new("pos", PortType::Vec2, PortDefault::Vec2(Vec2::ZERO)),
            PortMeta::scalar("rot", 0.0),
        ],
    ),
];

/// The catalog row of `kind`.
pub fn meta(kind: &NodeKind) -> &'static NodeMeta {
    let i = match kind {
        NodeKind::Time => 0,
        NodeKind::Constant => 1,
        NodeKind::Oscillator => 2,
        NodeKind::Key { .. } => 3,
        NodeKind::Param { .. } => 4,
        NodeKind::Gain => 5,
        NodeKind::Sum => 6,
        NodeKind::Product => 7,
        NodeKind::Sub => 8,
        NodeKind::Abs => 9,
        NodeKind::Min => 10,
        NodeKind::Max => 11,
        NodeKind::Curve => 12,
        NodeKind::Expr(_) => 13,
        NodeKind::Box => 14,
        NodeKind::Circle => 15,
        NodeKind::Polygon => 16,
        NodeKind::Capsule => 17,
        NodeKind::HalfPlane => 18,
        NodeKind::Csg(_) => 19,
        NodeKind::Placed => 20,
    };
    &CATALOG[i]
}

impl NodeKind {
    /// A node of the catalog row `id` with its fields at their defaults (a
    /// `[0, 1]` param at 0.5, the `E` key, a union), or `None` for an
    /// unknown id or for `signal::expr`, whose expression has no default
    /// (declare one with `defsignal`).
    pub fn from_catalog_id(id: &str) -> Option<Self> {
        Some(match id {
            "signal::time" => Self::Time,
            "signal::constant" => Self::Constant,
            "signal::oscillator" => Self::Oscillator,
            "signal::key" => Self::Key {
                key: "E".to_owned(),
                negative: None,
            },
            "signal::param" => Self::Param {
                value: 0.5,
                min: 0.0,
                max: 1.0,
            },
            "signal::gain" => Self::Gain,
            "signal::sum" => Self::Sum,
            "signal::product" => Self::Product,
            "signal::sub" => Self::Sub,
            "signal::abs" => Self::Abs,
            "signal::min" => Self::Min,
            "signal::max" => Self::Max,
            "signal::curve" => Self::Curve,
            "shape::box" => Self::Box,
            "shape::circle" => Self::Circle,
            "shape::polygon" => Self::Polygon,
            "shape::capsule" => Self::Capsule,
            "shape::half_plane" => Self::HalfPlane,
            "shape::csg" => Self::Csg(gradiance_domain::shape::CsgOp::Union),
            "shape::placed" => Self::Placed,
            _ => return None,
        })
    }
}

/// The text of `docs/nodes.md`: the catalog as one table per category, in
/// palette order. Generated; a test in this crate fails when the file is
/// stale and rewrites it with `UPDATE_NODES_DOC=1`.
pub fn nodes_markdown() -> String {
    use std::fmt::Write as _;
    let cell = |text: &str| text.replace('|', "\\|");
    let ports = |ports: &[PortMeta]| {
        if ports.is_empty() {
            return "—".to_owned();
        }
        ports
            .iter()
            .map(|p| {
                let hard = p.hard.map_or_else(String::new, |(lo, hi)| {
                    if hi == f32::MAX {
                        format!(", at least {lo}")
                    } else {
                        format!(", {lo} to {hi}")
                    }
                });
                format!("`{}`: {:?} = {}{hard}", p.name, p.ty, default_text(&p.default))
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "# Nodes\n\n\
         <!-- Generated from `gradiance_graph::CATALOG` by \
         `crates/gradiance-graph/src/tests.rs` (`docs_nodes_md_is_current`). Do not \
         edit by hand: run `UPDATE_NODES_DOC=1 cargo test -p gradiance-graph \
         docs_nodes_md_is_current`. -->\n\n\
         Every node kind of the one graph (`docs/graph-model-decision.md`): {} \
         rows. A node is not an operation; the `graph.*` operations \
         (`docs/ops.md`) edit the graph. **Rate**: a frame-rate node runs every \
         frame through the signal kernel; an edit-rate node changes only on a \
         settled edit, and an edit-rate input never takes a frame-rate output. \
         An input port reads its default while nothing is bound to it; a \
         constant must lie in its hard range.\n",
        CATALOG.len()
    );
    let mut category = "";
    for row in CATALOG {
        if row.category != category {
            category = row.category;
            let _ = write!(
                out,
                "\n## {category}\n\n\
                 | Id | Name | Rate | Inputs | Outputs | Doc |\n\
                 |---|---|---|---|---|---|\n"
            );
        }
        let rate = match row.rate {
            Rate::Frame => "frame",
            Rate::Edit => "edit",
        };
        let inputs = if matches!(row.id, "signal::expr") {
            "one scalar per name the expression reads".to_owned()
        } else {
            ports(row.inputs)
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {rate} | {} | {} | {} |",
            row.id,
            cell(row.name),
            cell(&inputs),
            cell(
                &row.outputs
                    .iter()
                    .map(|p| format!("`{}`: {:?}", p.name, p.ty))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            cell(row.doc),
        );
    }
    out
}

/// A port default as `docs/nodes.md` spells it.
fn default_text(default: &PortDefault) -> String {
    match default {
        PortDefault::Scalar(v) => format!("{v}"),
        PortDefault::Vec2(v) => format!("({}, {})", v.x, v.y),
        PortDefault::Points(points) => format!("{} points", points.len()),
        PortDefault::NoContours => "none".to_owned(),
        PortDefault::UnitBox => "1 m box".to_owned(),
        PortDefault::IdentityCurve => "identity".to_owned(),
    }
}
