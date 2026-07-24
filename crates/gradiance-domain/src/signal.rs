//! Signal-dataflow bindings: the persisted half of the signal substrate
//! (`docs/signal-dataflow.md`).
//!
//! These are the *data* types — sources, mapping, gradients, sinks, and
//! the [`SignalBindings`] config-seam resource that persists with the
//! scene (like [`settings`](crate::settings)). The runtime that
//! evaluates them (the bus, the derived color override, the per-frame
//! evaluator) lives in the `signal` layer crate.

use bevy::prelude::*;
use colorgrad::Gradient;
use gradiance_core::ids::StableId;
use serde::{Deserialize, Serialize};

/// Gradient lookups are quantized to this many bands so a continuously
/// varying signal only re-tints (and re-builds materials) when it crosses
/// a band, not every frame.
const GRADIENT_BANDS: f32 = 48.0;

/// Where a signal's value comes from — a *read* of scene state. Bodies are
/// referenced by [`StableId`] (never `Entity`); an unresolvable source
/// simply produces no sample this frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum SignalSource {
    /// Linear speed of a body (m/s).
    Speed(StableId),
    /// Angular speed of a body (rad/s, absolute).
    Spin(StableId),
    /// A body's height (world y, m).
    Height(StableId),
    /// A body's horizontal position (world x, m).
    PosX(StableId),
    /// Centre-to-centre distance between two bodies (m).
    Distance(StableId, StableId),
    /// Net normal contact force on a body (impulse / fixed dt).
    ContactForce(StableId),
    /// Number of bodies a body is currently touching.
    ContactCount(StableId),
    /// Total kinetic energy of a body (½mv² + ½Iω², J).
    KineticEnergy(StableId),
    /// Linear momentum magnitude of a body (m·|v|, kg·m/s).
    Momentum(StableId),
    /// A named value published on the `SignalBus` (in the `signal` layer)
    /// — by a script
    /// (`signal-set`), a future node, or another binding.
    Named(String),
}

/// Input-domain mapping: `value` is normalized to `t ∈ [0, 1]` over
/// `[in_min, in_max]` (clamped). Pure.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SignalMap {
    /// Value mapped to `t = 0`.
    pub in_min: f32,
    /// Value mapped to `t = 1`.
    pub in_max: f32,
}

impl Default for SignalMap {
    fn default() -> Self {
        Self {
            in_min: 0.0,
            in_max: 500.0,
        }
    }
}

impl SignalMap {
    /// Normalizes `value` into `[0, 1]` (degenerate domains map to 0).
    pub fn normalize(self, value: f32) -> f32 {
        let span = self.in_max - self.in_min;
        if span.abs() < f32::EPSILON {
            return 0.0;
        }
        ((value - self.in_min) / span).clamp(0.0, 1.0)
    }
}

/// How a [`Curve`] interpolates between its control points.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum CurveInterp {
    /// Straight segments between points.
    #[default]
    Linear,
    /// A smooth, non-overshooting monotone-cubic spline (the Lightroom feel).
    Smooth,
}

/// An authored **response curve** reshaping a normalized input `x ∈ [0, 1]` to
/// an output `y ∈ [0, 1]` — the Lightroom-style transfer the curve editor edits
/// and the signal path applies after [`SignalMap`]. Control points are `(x, y)`
/// ascending in `x`; the default identity line `(0,0)–(1,1)` is a no-op (today's
/// straight map), so it is fully backward-compatible. Pure; the hot path lowers
/// it to the Tier-B kernel (later), the cold binding path calls [`eval`](Self::eval).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Curve {
    /// Control points `(x, y)`, ascending in `x`; at least two.
    pub points: Vec<Vec2>,
    /// How to interpolate between them.
    pub interp: CurveInterp,
}

impl Default for Curve {
    fn default() -> Self {
        Self {
            points: vec![Vec2::ZERO, Vec2::ONE],
            interp: CurveInterp::Linear,
        }
    }
}

impl Curve {
    /// Evaluates the curve at `x`, clamped to the point range and to `[0, 1]`.
    /// Degenerate curves (fewer than two points) fall back to the identity.
    pub fn eval(&self, x: f32) -> f32 {
        let pts = &self.points;
        match pts.len() {
            0 => return x.clamp(0.0, 1.0),
            1 => return pts[0].y.clamp(0.0, 1.0),
            _ => {}
        }
        // Clamp outside the authored domain to the end points (flat holds).
        if x <= pts[0].x {
            return pts[0].y.clamp(0.0, 1.0);
        }
        if x >= pts[pts.len() - 1].x {
            return pts[pts.len() - 1].y.clamp(0.0, 1.0);
        }
        // The segment [i, i+1] bracketing x.
        let i = pts.iter().rposition(|p| p.x <= x).unwrap_or(0);
        let (p0, p1) = (pts[i], pts[i + 1]);
        let h = p1.x - p0.x;
        if h.abs() < f32::EPSILON {
            return p0.y.clamp(0.0, 1.0);
        }
        let s = (x - p0.x) / h;
        let y = match self.interp {
            CurveInterp::Linear => p0.y + s * (p1.y - p0.y),
            CurveInterp::Smooth => {
                // Monotone-cubic (Fritsch–Carlson) Hermite over this segment.
                let m0 = self.tangent_at(i);
                let m1 = self.tangent_at(i + 1);
                let (s2, s3) = (s * s, s * s * s);
                let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
                let h10 = s3 - 2.0 * s2 + s;
                let h01 = -2.0 * s3 + 3.0 * s2;
                let h11 = s3 - s2;
                h00 * p0.y + h10 * h * m0 + h01 * p1.y + h11 * h * m1
            }
        };
        y.clamp(0.0, 1.0)
    }

    /// The monotonicity-preserving tangent (slope) at control point `i`, from
    /// its adjacent secants (Fritsch–Carlson). Local, so `eval` stays cheap.
    fn tangent_at(&self, i: usize) -> f32 {
        let pts = &self.points;
        let secant = |a: usize, b: usize| {
            let dx = pts[b].x - pts[a].x;
            if dx.abs() < f32::EPSILON {
                0.0
            } else {
                (pts[b].y - pts[a].y) / dx
            }
        };
        let last = pts.len() - 1;
        if i == 0 {
            return secant(0, 1);
        }
        if i == last {
            return secant(last - 1, last);
        }
        let (left, right) = (secant(i - 1, i), secant(i, i + 1));
        // A local extremum → flat tangent, so the spline can't overshoot.
        if left * right <= 0.0 {
            0.0
        } else {
            0.5 * (left + right)
        }
    }
}

/// A named color gradient (via the `colorgrad` crate — no reinvented
/// wheels). Serializable so bindings persist; the node editor later swaps
/// this for user-authored gradients through the same `at()` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum GradientSpec {
    /// Perceptually uniform blue→green→yellow (the default).
    #[default]
    Viridis,
    /// High-contrast blue→red rainbow.
    Turbo,
    /// Purple→orange→yellow.
    Plasma,
    /// Black→red→yellow (heat).
    Inferno,
    /// Blue→white→red centered diverging ramp.
    CoolWarm,
}

impl GradientSpec {
    /// Every preset, for the UI combo.
    pub const ALL: [Self; 5] = [
        Self::Viridis,
        Self::Turbo,
        Self::Plasma,
        Self::Inferno,
        Self::CoolWarm,
    ];

    /// The gradient color at `t ∈ [0, 1]`, quantized to `GRADIENT_BANDS`.
    pub fn at(self, t: f32) -> Color {
        let t = (t.clamp(0.0, 1.0) * GRADIENT_BANDS).round() / GRADIENT_BANDS;
        let c = match self {
            Self::Viridis => colorgrad::preset::viridis().at(t),
            Self::Turbo => colorgrad::preset::turbo().at(t),
            Self::Plasma => colorgrad::preset::plasma().at(t),
            Self::Inferno => colorgrad::preset::inferno().at(t),
            Self::CoolWarm => colorgrad::preset::rd_bu().at(1.0 - t),
        };
        Color::srgba(c.r, c.g, c.b, 1.0)
    }
}

/// What a signal drives. Color sinks write the derived
/// `SignalColorOverride` (in the `signal` layer); `Plot`
/// only publishes (every binding's value
/// lands on the bus and in the plot panel regardless).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum SignalSink {
    /// Tint a body's fill (render override; authored appearance untouched).
    Fill(StableId),
    /// Tint a body's tracer trail.
    TracerColor(StableId),
    /// Publish/plot only.
    Plot,
}

/// One source → map → gradient → sink wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SignalBinding {
    /// Bus name the value publishes under (also the plot label).
    pub name: String,
    /// The read.
    pub source: SignalSource,
    /// Input domain → `[0, 1]`.
    pub map: SignalMap,
    /// Optional response curve reshaping the normalized `t` before the gradient
    /// (the Lightroom-style transfer). `None` is the straight identity, so old
    /// scenes without the field load unchanged.
    #[serde(default)]
    pub curve: Option<Curve>,
    /// Color ramp for color sinks.
    pub gradient: GradientSpec,
    /// The derived write.
    pub sink: SignalSink,
}

impl SignalBinding {
    /// The final transfer `t ∈ [0, 1]`: the normalized value reshaped by the
    /// optional [`Curve`]. The single place the map + curve compose, shared by
    /// the evaluator and any preview.
    pub fn transfer(&self, value: f32) -> f32 {
        let t = self.map.normalize(value);
        self.curve.as_ref().map_or(t, |c| c.eval(t))
    }
}

/// The signal graph: every active binding. Config-seam resource (UI edits
/// directly, persisted in the scene's `EnvironmentRecord`, not undoable).
#[derive(Resource, Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct SignalBindings(pub Vec<SignalBinding>);

/// Remaps a body id through `id_map` (old → new), returning the new id and
/// whether it changed.
fn remap_id(id: StableId, id_map: &[(StableId, StableId)]) -> (StableId, bool) {
    id_map
        .iter()
        .find(|(old, _)| *old == id)
        .map_or((id, false), |(_, new)| (*new, true))
}

impl SignalSource {
    /// The canonical bus name a scene sensor publishes under so the modulation
    /// tier (computed/blocks) can read it — `"speed@<uuid>"`. `None` for
    /// non-body sources (`Distance`, `Named`), which have no single sensor pin.
    pub fn bus_name(&self) -> Option<String> {
        let (tag, id) = match self {
            Self::Speed(id) => ("speed", id),
            Self::Spin(id) => ("spin", id),
            Self::Height(id) => ("height", id),
            Self::PosX(id) => ("posx", id),
            Self::ContactForce(id) => ("force", id),
            Self::ContactCount(id) => ("contacts", id),
            Self::KineticEnergy(id) => ("energy", id),
            Self::Momentum(id) => ("momentum", id),
            Self::Distance(..) | Self::Named(_) => return None,
        };
        Some(format!("{tag}@{}", id.0))
    }

    /// The physical [`Dimension`](gradiance_units::Dimension) this source
    /// reads — the P3 catalog binding (`docs/units-decision.md`) that gives
    /// every plot axis, port readout, and inspector row its unit label from
    /// one place instead of a hard-coded string per call site. `ContactCount`
    /// and a `Named` script value carry no declared dimension.
    pub fn dimension(&self) -> gradiance_units::Dimension {
        use gradiance_units::Dimension;
        match self {
            Self::Speed(_) => Dimension::Velocity,
            Self::Spin(_) => Dimension::AngularVelocity,
            Self::Height(_) | Self::PosX(_) | Self::Distance(..) => Dimension::Length,
            Self::ContactForce(_) => Dimension::Force,
            Self::KineticEnergy(_) => Dimension::Energy,
            Self::Momentum(_) => Dimension::Momentum,
            Self::ContactCount(_) | Self::Named(_) => Dimension::Dimensionless,
        }
    }

    /// Parses a canonical sensor bus name back to its source (inverse of
    /// [`bus_name`](Self::bus_name)).
    pub fn from_bus_name(name: &str) -> Option<Self> {
        let (tag, id) = name.split_once('@')?;
        let sid = StableId(uuid::Uuid::parse_str(id).ok()?);
        Some(match tag {
            "speed" => Self::Speed(sid),
            "spin" => Self::Spin(sid),
            "height" => Self::Height(sid),
            "posx" => Self::PosX(sid),
            "force" => Self::ContactForce(sid),
            "contacts" => Self::ContactCount(sid),
            "energy" => Self::KineticEnergy(sid),
            "momentum" => Self::Momentum(sid),
            _ => return None,
        })
    }

    /// Remaps body references through `id_map`; the bool is whether any
    /// reference belonged to the map (the source "touches" a remapped body).
    fn remapped(&self, id_map: &[(StableId, StableId)]) -> (Self, bool) {
        let one = |id: StableId| remap_id(id, id_map);
        match self {
            Self::Speed(a) => (Self::Speed(one(*a).0), one(*a).1),
            Self::Spin(a) => (Self::Spin(one(*a).0), one(*a).1),
            Self::Height(a) => (Self::Height(one(*a).0), one(*a).1),
            Self::PosX(a) => (Self::PosX(one(*a).0), one(*a).1),
            Self::ContactForce(a) => (Self::ContactForce(one(*a).0), one(*a).1),
            Self::ContactCount(a) => (Self::ContactCount(one(*a).0), one(*a).1),
            Self::KineticEnergy(a) => (Self::KineticEnergy(one(*a).0), one(*a).1),
            Self::Momentum(a) => (Self::Momentum(one(*a).0), one(*a).1),
            Self::Distance(a, b) => {
                let (ra, ta) = one(*a);
                let (rb, tb) = one(*b);
                (Self::Distance(ra, rb), ta || tb)
            }
            Self::Named(n) => (Self::Named(n.clone()), false),
        }
    }
}

impl SignalSink {
    /// Remaps body references through `id_map` (see [`SignalSource::remapped`]).
    fn remapped(&self, id_map: &[(StableId, StableId)]) -> (Self, bool) {
        match self {
            Self::Fill(a) => {
                let (r, t) = remap_id(*a, id_map);
                (Self::Fill(r), t)
            }
            Self::TracerColor(a) => {
                let (r, t) = remap_id(*a, id_map);
                (Self::TracerColor(r), t)
            }
            Self::Plot => (Self::Plot, false),
        }
    }
}

/// Clones `binding` with its body references remapped through `id_map`,
/// returning `Some` only when it references at least one remapped body — so
/// duplicating a body copies exactly the bindings that were about it
/// ("behavior copies with the base object"). The clone keeps `binding`'s
/// name; the caller assigns a fresh one.
pub fn remap_binding(
    binding: &SignalBinding,
    id_map: &[(StableId, StableId)],
) -> Option<SignalBinding> {
    let (source, s_touched) = binding.source.remapped(id_map);
    let (sink, k_touched) = binding.sink.remapped(id_map);
    (s_touched || k_touched).then(|| SignalBinding {
        name: binding.name.clone(),
        source,
        map: binding.map,
        curve: binding.curve.clone(),
        gradient: binding.gradient,
        sink,
    })
}

/// A tunable named parameter — the auto-slider a `defparam` produces
/// (the "knob" of the P2 driver layer, `docs/signal-dataflow.md`). Its
/// value is published on the bus each frame under `name`, and it is the
/// simplest **modulator input**: a slider a computed signal reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SignalParam {
    /// Bus name this parameter publishes under.
    pub name: String,
    /// Current value (the slider position).
    pub value: f32,
    /// Slider lower bound.
    pub min: f32,
    /// Slider upper bound.
    pub max: f32,
}

impl SignalParam {
    /// A `[0, 1]` parameter at `0.5` — the default a bare `defparam` gets.
    pub fn unit(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: 0.5,
            min: 0.0,
            max: 1.0,
        }
    }
}

/// A serializable numeric expression over **named** signals — the authored
/// form of a computed signal (`defsignal`). It mirrors the pure Tier-B
/// kernel's `Expr` (`gradiance_kernel`) but references inputs by bus
/// name instead of a var index, so it persists and round-trips; the signal
/// runtime lowers it to a compiled, allocation-free `Kernel` for the hot
/// path (`gradiance_signal::compile`). Kept small on purpose — power comes
/// from composition, not a big opcode set.
///
/// Reflects as an **opaque** value (like `ShapeDef`): it is a recursive
/// `Box`-ed tree, addressed by scripts/persistence as a whole rather than
/// field-walked. Serialize/Deserialize (persistence) are independent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(opaque, Serialize, Deserialize)]
pub enum SignalExpr {
    /// A literal constant.
    Const(f32),
    /// The current value of a bus signal (a param, a binding, another
    /// computed signal, or the special input `"t"` = elapsed seconds).
    Input(String),
    /// Negate.
    Neg(Box<SignalExpr>),
    /// Sine (radians).
    Sin(Box<SignalExpr>),
    /// Cosine (radians).
    Cos(Box<SignalExpr>),
    /// Absolute value.
    Abs(Box<SignalExpr>),
    /// Add.
    Add(Box<SignalExpr>, Box<SignalExpr>),
    /// Subtract.
    Sub(Box<SignalExpr>, Box<SignalExpr>),
    /// Multiply.
    Mul(Box<SignalExpr>, Box<SignalExpr>),
    /// Divide.
    Div(Box<SignalExpr>, Box<SignalExpr>),
    /// Minimum.
    Min(Box<SignalExpr>, Box<SignalExpr>),
    /// Maximum.
    Max(Box<SignalExpr>, Box<SignalExpr>),
}

impl SignalExpr {
    /// Collects the distinct bus-input names this expression references,
    /// in first-seen order (the column order the compiled kernel expects).
    pub fn inputs(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_inputs(&mut out);
        out
    }

    /// Parses a whitespace-separated **reverse-Polish** expression into a
    /// tree: numbers are constants, the operators `+ - * / min max` (binary)
    /// and `neg sin cos abs` (unary) combine the stack, and any other token
    /// is a named [`SignalExpr::Input`] (a bus signal, a param, or `"t"`).
    /// RPN keeps the surface tiny and unambiguous while the node editor is
    /// still text — e.g. `"t sin amp *"` is `amp * sin(t)`.
    pub fn parse_rpn(source: &str) -> Result<Self, SignalExprError> {
        let mut stack: Vec<SignalExpr> = Vec::new();
        for token in source.split_whitespace() {
            let mut binary = |op: fn(Box<Self>, Box<Self>) -> Self| -> Result<(), SignalExprError> {
                let b = stack.pop().ok_or(SignalExprError::Underflow)?;
                let a = stack.pop().ok_or(SignalExprError::Underflow)?;
                stack.push(op(Box::new(a), Box::new(b)));
                Ok(())
            };
            match token {
                "+" => binary(Self::Add)?,
                "-" => binary(Self::Sub)?,
                "*" => binary(Self::Mul)?,
                "/" => binary(Self::Div)?,
                "min" => binary(Self::Min)?,
                "max" => binary(Self::Max)?,
                "neg" | "sin" | "cos" | "abs" => {
                    let a = stack.pop().ok_or(SignalExprError::Underflow)?;
                    let boxed = Box::new(a);
                    stack.push(match token {
                        "neg" => Self::Neg(boxed),
                        "sin" => Self::Sin(boxed),
                        "cos" => Self::Cos(boxed),
                        _ => Self::Abs(boxed),
                    });
                }
                other => stack.push(match other.parse::<f32>() {
                    Ok(c) => Self::Const(c),
                    Err(_) => Self::Input(other.to_owned()),
                }),
            }
        }
        match stack.pop() {
            Some(expr) if stack.is_empty() => Ok(expr),
            Some(_) => Err(SignalExprError::Leftover(stack.len() + 1)),
            None => Err(SignalExprError::Empty),
        }
    }

    fn collect_inputs(&self, out: &mut Vec<String>) {
        match self {
            Self::Const(_) => {}
            Self::Input(name) => {
                if !out.iter().any(|n| n == name) {
                    out.push(name.clone());
                }
            }
            Self::Neg(a) | Self::Sin(a) | Self::Cos(a) | Self::Abs(a) => a.collect_inputs(out),
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::Min(a, b)
            | Self::Max(a, b) => {
                a.collect_inputs(out);
                b.collect_inputs(out);
            }
        }
    }
}

/// Why a reverse-Polish expression could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignalExprError {
    /// An operator ran out of operands on the stack.
    #[error("expression underflow: an operator had too few operands")]
    Underflow,
    /// The expression was empty.
    #[error("empty expression")]
    Empty,
    /// More than one value was left on the stack (missing operator).
    #[error("expression left {0} values on the stack (expected 1)")]
    Leftover(usize),
}

/// A computed signal (`defsignal`): a named value that is a numeric
/// **modulator** over other signals, evaluated every frame through the
/// compiled kernel and published on the bus under `name`. This is the
/// middle tier of the sensor→modulator→actuator model — the piece that
/// makes the dataflow a *graph* rather than a set of direct wires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ComputedSignal {
    /// Bus name this computed value publishes under.
    pub name: String,
    /// The expression, over other bus signals (and `"t"`). Always the form
    /// the kernel compiles; for a canvas block it is *derived* from `block`.
    pub expr: SignalExpr,
    /// The structured node-editor block this signal came from, if authored on
    /// the canvas (`None` = a console `defsignal` RPN). Drives the block's pins
    /// and is re-lowered to `expr` when its operands change.
    #[serde(default)]
    pub block: Option<BlockOp>,
}

impl ComputedSignal {
    /// A canvas block signal — `expr` lowered from `op`.
    pub fn from_block(name: impl Into<String>, op: BlockOp) -> Self {
        Self {
            name: name.into(),
            expr: op.to_expr(),
            block: Some(op),
        }
    }

    /// Re-lowers `expr` from the current `block` (call after editing operands).
    pub fn relower(&mut self) {
        if let Some(op) = &self.block {
            self.expr = op.to_expr();
        }
    }
}

/// A structured node-editor block, lowered to a [`SignalExpr`] for the kernel.
/// Input blocks are sources (no signal inputs); modulation blocks combine
/// named bus signals through their operand slots. Wiring a producer into a slot
/// sets that operand's bus name; the block re-lowers to `expr`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum BlockOp {
    /// The elapsed-seconds clock `t` (an input/source block).
    Time,
    /// A fixed value — the plainest source block (a Simulink Constant).
    Constant {
        /// The constant this block emits.
        value: f32,
    },
    /// `amp · sin(freq · t)` — a self-driving oscillator source.
    Oscillator {
        /// Amplitude.
        amp: f32,
        /// Frequency (radians per second of `t`).
        freq: f32,
    },
    /// `in · k` — a scalar gain (one signal input + a constant).
    Gain {
        /// The bus name feeding the input (`None` = unconnected → 0).
        input: Option<String>,
        /// Gain constant.
        k: f32,
    },
    /// `a + b` — a summing junction (two signal inputs).
    Sum {
        /// First operand's bus name.
        a: Option<String>,
        /// Second operand's bus name.
        b: Option<String>,
    },
    /// `a · b` — a product (two signal inputs).
    Product {
        /// First operand's bus name.
        a: Option<String>,
        /// Second operand's bus name.
        b: Option<String>,
    },
    /// `a − b` — a difference (two signal inputs).
    Sub {
        /// Minuend's bus name.
        a: Option<String>,
        /// Subtrahend's bus name.
        b: Option<String>,
    },
    /// `|in|` — absolute value (one signal input).
    Abs {
        /// The bus name feeding the input.
        input: Option<String>,
    },
    /// `min(a, b)` — the lesser of two signal inputs.
    Min {
        /// First operand's bus name.
        a: Option<String>,
        /// Second operand's bus name.
        b: Option<String>,
    },
    /// `max(a, b)` — the greater of two signal inputs.
    Max {
        /// First operand's bus name.
        a: Option<String>,
        /// Second operand's bus name.
        b: Option<String>,
    },
}

impl BlockOp {
    /// Lowers the block to the kernel expression. An unconnected operand reads
    /// `0` (a disconnected wire contributes nothing).
    pub fn to_expr(&self) -> SignalExpr {
        let operand = |name: &Option<String>| {
            name.as_ref()
                .map_or(SignalExpr::Const(0.0), |n| SignalExpr::Input(n.clone()))
        };
        let clock = || Box::new(SignalExpr::Input("t".to_owned()));
        match self {
            Self::Time => SignalExpr::Input("t".to_owned()),
            Self::Constant { value } => SignalExpr::Const(*value),
            Self::Oscillator { amp, freq } => SignalExpr::Mul(
                Box::new(SignalExpr::Const(*amp)),
                Box::new(SignalExpr::Sin(Box::new(SignalExpr::Mul(
                    Box::new(SignalExpr::Const(*freq)),
                    clock(),
                )))),
            ),
            Self::Gain { input, k } => {
                SignalExpr::Mul(Box::new(operand(input)), Box::new(SignalExpr::Const(*k)))
            }
            Self::Sum { a, b } => SignalExpr::Add(Box::new(operand(a)), Box::new(operand(b))),
            Self::Product { a, b } => SignalExpr::Mul(Box::new(operand(a)), Box::new(operand(b))),
            Self::Sub { a, b } => SignalExpr::Sub(Box::new(operand(a)), Box::new(operand(b))),
            Self::Abs { input } => SignalExpr::Abs(Box::new(operand(input))),
            Self::Min { a, b } => SignalExpr::Min(Box::new(operand(a)), Box::new(operand(b))),
            Self::Max { a, b } => SignalExpr::Max(Box::new(operand(a)), Box::new(operand(b))),
        }
    }

    /// The signal-input slots `(label, connected name)` this block exposes as
    /// input pins — empty for source blocks.
    pub fn input_slots(&self) -> Vec<(&'static str, Option<String>)> {
        match self {
            Self::Time | Self::Constant { .. } | Self::Oscillator { .. } => Vec::new(),
            Self::Gain { input, .. } | Self::Abs { input } => vec![("in", input.clone())],
            Self::Sum { a, b }
            | Self::Product { a, b }
            | Self::Sub { a, b }
            | Self::Min { a, b }
            | Self::Max { a, b } => {
                vec![("a", a.clone()), ("b", b.clone())]
            }
        }
    }

    /// Sets input slot `i` to `name` (`None` = disconnect).
    pub fn set_input(&mut self, i: usize, name: Option<String>) {
        match self {
            Self::Time | Self::Constant { .. } | Self::Oscillator { .. } => {}
            Self::Gain { input, .. } | Self::Abs { input } => {
                if i == 0 {
                    *input = name;
                }
            }
            Self::Sum { a, b }
            | Self::Product { a, b }
            | Self::Sub { a, b }
            | Self::Min { a, b }
            | Self::Max { a, b } => match i {
                0 => *a = name,
                1 => *b = name,
                _ => {}
            },
        }
    }

    /// A short type label for the block palette / title.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Constant { .. } => "constant",
            Self::Oscillator { .. } => "oscillator",
            Self::Gain { .. } => "gain",
            Self::Sum { .. } => "sum",
            Self::Product { .. } => "product",
            Self::Sub { .. } => "sub",
            Self::Abs { .. } => "abs",
            Self::Min { .. } => "min",
            Self::Max { .. } => "max",
        }
    }
}

/// Every tunable parameter. Config-seam resource (UI slider edits directly,
/// persisted with the scene, not undoable).
#[derive(Resource, Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct SignalParams(pub Vec<SignalParam>);

/// Every computed signal. Config-seam resource, persisted with the scene.
#[derive(Resource, Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct ComputedSignals(pub Vec<ComputedSignal>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_map_normalizes_and_clamps() {
        let map = SignalMap {
            in_min: 100.0,
            in_max: 300.0,
        };
        assert!(map.normalize(100.0).abs() < 1e-6);
        assert!((map.normalize(300.0) - 1.0).abs() < 1e-6);
        assert!((map.normalize(200.0) - 0.5).abs() < 1e-6);
        assert!(map.normalize(-50.0).abs() < 1e-6, "clamps below");
        assert!((map.normalize(900.0) - 1.0).abs() < 1e-6, "clamps above");
        let degenerate = SignalMap {
            in_min: 5.0,
            in_max: 5.0,
        };
        assert!(degenerate.normalize(9.0).abs() < 1e-6);
    }

    #[test]
    fn the_default_curve_is_the_identity() {
        let id = Curve::default();
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((id.eval(x) - x).abs() < 1e-6, "identity at {x}");
        }
    }

    #[test]
    fn a_linear_curve_interpolates_and_holds_at_the_ends() {
        // An inverting ramp with a kink: (0,1)-(0.5,0.5)-(1,0).
        let curve = Curve {
            points: vec![
                Vec2::new(0.0, 1.0),
                Vec2::new(0.5, 0.5),
                Vec2::new(1.0, 0.0),
            ],
            interp: CurveInterp::Linear,
        };
        assert!((curve.eval(0.0) - 1.0).abs() < 1e-6);
        assert!((curve.eval(0.25) - 0.75).abs() < 1e-6, "lerp in segment 0");
        assert!((curve.eval(0.5) - 0.5).abs() < 1e-6, "the kink");
        assert!((curve.eval(0.75) - 0.25).abs() < 1e-6, "lerp in segment 1");
        // Outside the authored x-range holds flat at the end value.
        assert!((curve.eval(-1.0) - 1.0).abs() < 1e-6);
        assert!(curve.eval(2.0).abs() < 1e-6);
    }

    #[test]
    fn a_smooth_curve_passes_through_points_and_never_overshoots() {
        // A monotone step; the smooth spline must stay within [0, 1] (no
        // Fritsch–Carlson overshoot) and hit every control point.
        let curve = Curve {
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(0.5, 0.9),
                Vec2::new(1.0, 1.0),
            ],
            interp: CurveInterp::Smooth,
        };
        for p in &curve.points {
            assert!((curve.eval(p.x) - p.y).abs() < 1e-5, "passes through {p:?}");
        }
        for i in 0..=100 {
            let y = curve.eval(i as f32 / 100.0);
            assert!((0.0..=1.0).contains(&y), "no overshoot at {i} → {y}");
        }
    }

    #[test]
    fn a_binding_composes_its_map_then_curve() {
        // Map [0,400] then an inverting curve: a fast value → a low transfer.
        let binding = SignalBinding {
            name: "b".into(),
            source: SignalSource::Speed(StableId::new()),
            map: SignalMap {
                in_min: 0.0,
                in_max: 400.0,
            },
            curve: Some(Curve {
                points: vec![Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)],
                interp: CurveInterp::Linear,
            }),
            gradient: GradientSpec::default(),
            sink: SignalSink::Plot,
        };
        // 400 → normalized 1.0 → curve → 0.0; 0 → 0.0 → curve → 1.0.
        assert!(binding.transfer(400.0).abs() < 1e-6);
        assert!((binding.transfer(0.0) - 1.0).abs() < 1e-6);
        assert!((binding.transfer(200.0) - 0.5).abs() < 1e-6, "midpoint");
    }

    #[test]
    fn signal_expr_collects_inputs_in_first_seen_order() {
        // (speed * sin(t)) + speed  → inputs [speed, t], deduped.
        let expr = SignalExpr::Add(
            Box::new(SignalExpr::Mul(
                Box::new(SignalExpr::Input("speed".into())),
                Box::new(SignalExpr::Sin(Box::new(SignalExpr::Input("t".into())))),
            )),
            Box::new(SignalExpr::Input("speed".into())),
        );
        assert_eq!(expr.inputs(), vec!["speed".to_string(), "t".to_string()]);
    }

    #[test]
    fn parse_rpn_builds_the_expected_tree() {
        // "t sin amp *" = amp * sin(t).
        let expr = SignalExpr::parse_rpn("t sin amp *").unwrap();
        assert_eq!(
            expr,
            SignalExpr::Mul(
                Box::new(SignalExpr::Sin(Box::new(SignalExpr::Input("t".into())))),
                Box::new(SignalExpr::Input("amp".into())),
            )
        );
        assert_eq!(
            SignalExpr::parse_rpn("3.5").unwrap(),
            SignalExpr::Const(3.5)
        );
        assert_eq!(SignalExpr::parse_rpn(""), Err(SignalExprError::Empty));
        assert_eq!(SignalExpr::parse_rpn("+"), Err(SignalExprError::Underflow));
        assert_eq!(
            SignalExpr::parse_rpn("1 2"),
            Err(SignalExprError::Leftover(2))
        );
    }

    #[test]
    fn gradients_span_distinct_endpoint_colors() {
        for spec in GradientSpec::ALL {
            let lo = spec.at(0.0);
            let hi = spec.at(1.0);
            assert_ne!(lo, hi, "{spec:?} endpoints are distinct");
            // Quantization is stable: values in the same band agree.
            assert_eq!(spec.at(0.500), spec.at(0.505));
        }
    }

    #[test]
    fn sensor_bus_names_round_trip() {
        let id = StableId::new();
        for source in [
            SignalSource::Speed(id),
            SignalSource::Spin(id),
            SignalSource::Height(id),
            SignalSource::PosX(id),
            SignalSource::ContactForce(id),
            SignalSource::ContactCount(id),
            SignalSource::KineticEnergy(id),
            SignalSource::Momentum(id),
        ] {
            let name = source.bus_name().expect("has a bus name");
            assert_eq!(SignalSource::from_bus_name(&name), Some(source));
        }
        // Non-body sources have no canonical name.
        assert_eq!(SignalSource::Named("x".into()).bus_name(), None);
        assert_eq!(SignalSource::from_bus_name("nonsense"), None);
    }

    #[test]
    fn source_dimensions_carry_si_units() {
        use gradiance_units::Dimension;
        let id = StableId::new();
        // The catalog binding: each source's unit label comes from its
        // dimension (SI now — never "px/s"/"px" again).
        assert_eq!(SignalSource::Speed(id).dimension(), Dimension::Velocity);
        assert_eq!(SignalSource::Speed(id).dimension().symbol(), "m/s");
        assert_eq!(
            SignalSource::Spin(id).dimension(),
            Dimension::AngularVelocity
        );
        assert_eq!(SignalSource::Height(id).dimension().symbol(), "m");
        assert_eq!(SignalSource::PosX(id).dimension().symbol(), "m");
        assert_eq!(
            SignalSource::Distance(id, id).dimension().symbol(),
            "m",
            "distance is a length"
        );
        assert_eq!(SignalSource::ContactForce(id).dimension().symbol(), "N");
        assert_eq!(SignalSource::KineticEnergy(id).dimension().symbol(), "J");
        assert_eq!(SignalSource::Momentum(id).dimension().symbol(), "kg·m/s");
        // A count and an unlabelled script value have no unit.
        assert_eq!(SignalSource::ContactCount(id).dimension().symbol(), "");
        assert_eq!(SignalSource::Named("x".into()).dimension().symbol(), "");
    }

    #[test]
    fn block_ops_lower_to_the_expected_kernel_expr() {
        // A sum of two named signals.
        let sum = BlockOp::Sum {
            a: Some("x".into()),
            b: Some("y".into()),
        };
        assert_eq!(
            sum.to_expr(),
            SignalExpr::Add(
                Box::new(SignalExpr::Input("x".into())),
                Box::new(SignalExpr::Input("y".into())),
            )
        );
        // Gain with an unconnected input reads 0.
        let gain = BlockOp::Gain {
            input: None,
            k: 2.0,
        };
        assert_eq!(
            gain.to_expr(),
            SignalExpr::Mul(
                Box::new(SignalExpr::Const(0.0)),
                Box::new(SignalExpr::Const(2.0)),
            )
        );
        // Wiring an operand re-lowers and shows in inputs().
        let mut g = BlockOp::Gain {
            input: None,
            k: 3.0,
        };
        g.set_input(0, Some("speed".into()));
        assert_eq!(g.input_slots(), vec![("in", Some("speed".to_string()))]);
        assert_eq!(g.to_expr().inputs(), vec!["speed".to_string()]);

        // A Constant is a source (no input pins) lowering to a bare const.
        let c = BlockOp::Constant { value: 4.5 };
        assert_eq!(c.to_expr(), SignalExpr::Const(4.5));
        assert!(c.input_slots().is_empty());
        assert!(c.to_expr().inputs().is_empty());
    }
}
