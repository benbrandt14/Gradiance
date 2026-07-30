//! Spike 2: the Tier-B driver kernel.
//!
//! De-risks the performance claim in `docs/script-lisp-decision.md`: driver
//! expressions must be evaluable at particle/fluid scale, which forbids the
//! scripting VM from ever entering the per-frame inner loop. The answer is a
//! two-step lowering:
//!
//! 1. author / compile-time: an [`Expr`] tree (the numeric DSL subset) is
//!    compiled once into a flat [`Kernel`] tape via [`Kernel::compile`];
//! 2. runtime / hot path: [`Kernel::eval`] walks the tape over a fixed
//!    stack — no recursion, no dynamic dispatch, no heap — and
//!    [`Kernel::drive`] applies it across columnar (structure-of-arrays)
//!    data with a single reused variable buffer, so a whole population is
//!    updated with **zero allocation in the loop**.
//!
//! The [`Expr`] tree is what the steel front-end (Tier A) will *produce and
//! compile*; nothing here depends on steel, bevy, or the ECS — this is pure,
//! proptested math, in the spirit of the `geometry` module.
//!
//! ```
//! use gradiance_kernel::{BinaryOp, Expr, Kernel, UnaryOp};
//!
//! // position offset = amplitude * sin(t): amplitude is per-element var 1,
//! // t is the broadcast scalar var 0.
//! let expr = Expr::binary(
//!     BinaryOp::Mul,
//!     Expr::var(1),
//!     Expr::unary(UnaryOp::Sin, Expr::var(0)),
//! );
//! let kernel = Kernel::compile(&expr).expect("compiles");
//!
//! let amplitude = [2.0_f32, 5.0];
//! let mut out = [0.0_f32; 2];
//! kernel.drive(std::f32::consts::FRAC_PI_2, &[&amplitude], &mut out);
//! assert!((out[0] - 2.0).abs() < 1e-6); // 2 * sin(π/2) = 2
//! assert!((out[1] - 5.0).abs() < 1e-6); // 5 * sin(π/2) = 5
//! ```

/// Largest evaluation-stack depth a [`Kernel`] may require.
///
/// The tape is validated against this at [`compile`](Kernel::compile) time,
/// so [`eval`](Kernel::eval) indexes the fixed stack within a proven bound
/// and never needs to allocate or risk overflow. Expression trees deep
/// enough to exceed it are rejected up front (they are far beyond any
/// hand-authored or node-graph driver).
pub const MAX_STACK: usize = 64;

/// Largest number of input variables (`t` plus per-element columns) a kernel
/// may read. Var index 0 is conventionally the broadcast scalar (time).
pub const MAX_VARS: usize = 16;

/// A unary numeric operator in the driver DSL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Arithmetic negation.
    Neg,
    /// Sine (radians).
    Sin,
    /// Cosine (radians).
    Cos,
    /// Square root (of the absolute value, to stay total).
    Sqrt,
    /// Absolute value.
    Abs,
}

impl UnaryOp {
    #[inline]
    fn apply(self, a: f32) -> f32 {
        match self {
            Self::Neg => -a,
            Self::Sin => a.sin(),
            Self::Cos => a.cos(),
            Self::Sqrt => a.abs().sqrt(),
            Self::Abs => a.abs(),
        }
    }
}

/// A binary numeric operator in the driver DSL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division (by zero yields a non-finite value, caught by driver
    /// observability rather than panicking).
    Div,
    /// Minimum of the two operands.
    Min,
    /// Maximum of the two operands.
    Max,
}

impl BinaryOp {
    #[inline]
    fn apply(self, a: f32, b: f32) -> f32 {
        match self {
            Self::Add => a + b,
            Self::Sub => a - b,
            Self::Mul => a * b,
            Self::Div => a / b,
            Self::Min => a.min(b),
            Self::Max => a.max(b),
        }
    }
}

/// A **lookup table** over `[0, 1]`: the Tier-B form of an authored response
/// curve.
///
/// The authoring side (a Lightroom-style curve of control points, monotone
/// cubic or linear) is far too shapeful to encode as opcodes, and evaluating
/// it directly means a per-sample binary search over a `Vec` — allocation-free
/// but branchy, and it drags the authoring representation into the hot loop,
/// which is exactly what the two-tier rule forbids. Sampling it once at
/// compile time into a uniform table reduces the hot path to a clamp, a
/// multiply, and a lerp, and makes every curve cost the same regardless of how
/// many points the user placed.
///
/// The table has at least two entries; input is clamped to `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Lut {
    samples: Vec<f32>,
}

impl Lut {
    /// Builds a table by sampling `f` uniformly over `[0, 1]`.
    ///
    /// `resolution` is the number of *intervals*; the table gets
    /// `resolution + 1` entries. Values below 1 are raised to 1, so the table
    /// always has the two endpoints a lerp needs.
    pub fn sample(resolution: usize, f: impl Fn(f32) -> f32) -> Self {
        let n = resolution.max(1);
        Self {
            samples: (0..=n).map(|i| f(i as f32 / n as f32)).collect(),
        }
    }

    /// Builds a table from precomputed samples (at least two; a shorter slice
    /// is padded by repetition so lookup stays total).
    pub fn from_samples(samples: &[f32]) -> Self {
        match samples {
            [] => Self {
                samples: vec![0.0, 0.0],
            },
            [only] => Self {
                samples: vec![*only, *only],
            },
            rest => Self {
                samples: rest.to_vec(),
            },
        }
    }

    /// Looks `x` up, clamping to `[0, 1]` and lerping between neighbours.
    #[inline]
    pub fn lookup(&self, x: f32) -> f32 {
        let n = self.samples.len() - 1;
        let pos = x.clamp(0.0, 1.0) * n as f32;
        let i = (pos as usize).min(n - 1);
        let frac = pos - i as f32;
        let a = self.samples[i];
        let b = self.samples[i + 1];
        a + frac * (b - a)
    }

    /// The table's entries — for tests and diagnostics.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

/// A numeric driver expression: the authored, structural form.
///
/// This is the subset a driver or a node-graph signal compiles down from.
/// It is deliberately tiny — leaves plus unary/binary ops — because the
/// expressive power comes from *composition* and from the variable columns,
/// not from a large opcode set.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal constant.
    Const(f32),
    /// A read of input variable `index` (0 = broadcast scalar, e.g. time;
    /// higher indices are per-element columns).
    Var(u8),
    /// A unary operation applied to a subexpression.
    Unary(UnaryOp, Box<Expr>),
    /// A binary operation over two subexpressions.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// A [`Lut`] lookup of a subexpression — a sampled response curve.
    Curve(Box<Expr>, Lut),
}

impl Expr {
    /// Convenience constructor for a constant leaf.
    pub fn constant(value: f32) -> Self {
        Self::Const(value)
    }

    /// Convenience constructor for a variable-read leaf.
    pub fn var(index: u8) -> Self {
        Self::Var(index)
    }

    /// Convenience constructor for a unary node.
    pub fn unary(op: UnaryOp, child: Expr) -> Self {
        Self::Unary(op, Box::new(child))
    }

    /// Convenience constructor for a binary node.
    pub fn binary(op: BinaryOp, lhs: Expr, rhs: Expr) -> Self {
        Self::Binary(op, Box::new(lhs), Box::new(rhs))
    }

    /// Reference (tree-walking) evaluator — the readable oracle the
    /// compiled [`Kernel`] is proptested against. Test-only: the hot path
    /// is [`Kernel::eval`]; nothing at runtime walks the tree.
    #[cfg(test)]
    pub(crate) fn eval_ref(&self, vars: &[f32]) -> f32 {
        match self {
            Self::Const(c) => *c,
            Self::Var(i) => vars.get(usize::from(*i)).copied().unwrap_or(0.0),
            Self::Unary(op, child) => op.apply(child.eval_ref(vars)),
            Self::Binary(op, lhs, rhs) => op.apply(lhs.eval_ref(vars), rhs.eval_ref(vars)),
            Self::Curve(child, lut) => lut.lookup(child.eval_ref(vars)),
        }
    }

    /// Highest variable index referenced, if any (used to size inputs).
    pub fn max_var(&self) -> Option<u8> {
        match self {
            Self::Const(_) => None,
            Self::Var(i) => Some(*i),
            Self::Unary(_, child) | Self::Curve(child, _) => child.max_var(),
            Self::Binary(_, lhs, rhs) => match (lhs.max_var(), rhs.max_var()) {
                (a, None) => a,
                (None, b) => b,
                (Some(a), Some(b)) => Some(a.max(b)),
            },
        }
    }
}

/// Why an [`Expr`] could not be compiled into a [`Kernel`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KernelError {
    /// The expression's evaluation stack would exceed [`MAX_STACK`].
    #[error("expression too deep: needs stack depth {needed}, max is {MAX_STACK}")]
    TooDeep {
        /// The stack depth the tape would require.
        needed: usize,
    },
    /// The expression reads a variable index at or beyond [`MAX_VARS`].
    #[error("variable index {index} out of range (max {MAX_VARS})")]
    VarOutOfRange {
        /// The offending variable index.
        index: u8,
    },
}

/// One instruction of the flat evaluation tape.
///
/// A postfix (reverse-Polish) encoding of the [`Expr`] tree: evaluating the
/// tape left-to-right against a stack reproduces the tree's value without
/// recursion or pointer chasing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Instr {
    Const(f32),
    Var(u8),
    Unary(UnaryOp),
    Binary(BinaryOp),
    /// Replace the top of stack with `luts[index].lookup(top)`. The table is
    /// held out-of-line so `Instr` stays `Copy` and the tape stays a flat
    /// array of small values.
    Curve(u16),
}

/// A compiled driver expression: a flat tape plus its proven stack depth.
///
/// Built once by [`compile`](Kernel::compile) (Tier A / cold path), then
/// evaluated many times by [`eval`](Kernel::eval) / [`drive`](Kernel::drive)
/// (Tier B / hot path) with no allocation.
#[derive(Debug, Clone)]
pub struct Kernel {
    tape: Vec<Instr>,
    /// Sampled response curves, referenced by [`Instr::Curve`] index.
    luts: Vec<Lut>,
    /// Peak stack depth this tape reaches (≤ [`MAX_STACK`]); validated in
    /// [`compile`](Kernel::compile), retained only for test diagnostics.
    #[cfg(test)]
    stack_depth: usize,
}

impl Kernel {
    /// Compiles an [`Expr`] tree into a flat tape.
    ///
    /// Fails if the tree would need a stack deeper than [`MAX_STACK`] or
    /// references a variable index at or beyond [`MAX_VARS`] — both checked
    /// here so the hot path can assume validity.
    pub fn compile(expr: &Expr) -> Result<Self, KernelError> {
        if let Some(max) = expr.max_var()
            && usize::from(max) >= MAX_VARS
        {
            return Err(KernelError::VarOutOfRange { index: max });
        }
        let mut tape = Vec::new();
        let mut luts = Vec::new();
        let depth = lower(expr, &mut tape, &mut luts, 0, 0);
        if depth > MAX_STACK {
            return Err(KernelError::TooDeep { needed: depth });
        }
        Ok(Self {
            tape,
            luts,
            #[cfg(test)]
            stack_depth: depth,
        })
    }

    /// The peak evaluation-stack depth this kernel reaches (test/report
    /// diagnostics only).
    #[cfg(test)]
    pub(crate) fn stack_depth(&self) -> usize {
        self.stack_depth
    }

    /// The number of tape instructions (a proxy for evaluation cost).
    pub fn len(&self) -> usize {
        self.tape.len()
    }

    /// Whether the tape is empty (never true for a compiled expression).
    pub fn is_empty(&self) -> bool {
        self.tape.is_empty()
    }

    /// Evaluates the tape against `vars` — the hot path.
    ///
    /// No recursion, no dynamic dispatch, no allocation: a fixed
    /// [`MAX_STACK`] scratch array lives on the call frame, and compilation
    /// already proved the tape never overflows it. Out-of-range variable
    /// reads (impossible for a compiled kernel, since [`compile`](Kernel::compile)
    /// bounds them) fall back to `0.0` rather than panicking.
    #[inline]
    pub fn eval(&self, vars: &[f32]) -> f32 {
        let mut stack = [0.0_f32; MAX_STACK];
        let mut sp = 0_usize;
        for instr in &self.tape {
            match *instr {
                Instr::Const(c) => {
                    stack[sp] = c;
                    sp += 1;
                }
                Instr::Var(i) => {
                    stack[sp] = vars.get(usize::from(i)).copied().unwrap_or(0.0);
                    sp += 1;
                }
                Instr::Unary(op) => {
                    // A unary op consumes and replaces the top of stack.
                    stack[sp - 1] = op.apply(stack[sp - 1]);
                }
                Instr::Binary(op) => {
                    let b = stack[sp - 1];
                    let a = stack[sp - 2];
                    sp -= 1;
                    stack[sp - 1] = op.apply(a, b);
                }
                Instr::Curve(index) => {
                    // Like a unary op: consumes and replaces the top of stack.
                    // The index is in range by construction (`lower` pushed the
                    // table); an out-of-range one would mean a corrupted tape,
                    // so fall through to the identity rather than panicking.
                    if let Some(lut) = self.luts.get(usize::from(index)) {
                        stack[sp - 1] = lut.lookup(stack[sp - 1]);
                    }
                }
            }
        }
        stack[0]
    }

    /// Applies the kernel across a population, writing one output per element.
    ///
    /// `t` is the broadcast scalar (variable 0). Each slice in `columns`
    /// supplies a per-element variable, mapped to variable index `1 + its
    /// position` — so `columns[0]` is variable 1, and so on. Every column and
    /// `out` must have the same length.
    ///
    /// This is the data-parallel shape the driver seam will run inside one
    /// ECS system: a single reused `vars` buffer, no per-element heap traffic.
    /// It is written as a straight loop so the optimizer (and, later, a
    /// SIMD/compute port) has nothing to fight.
    pub fn drive(&self, t: f32, columns: &[&[f32]], out: &mut [f32]) {
        debug_assert!(columns.len() < MAX_VARS, "too many columns");
        let mut vars = [0.0_f32; MAX_VARS];
        vars[0] = t;
        for (i, slot) in out.iter_mut().enumerate() {
            for (c, col) in columns.iter().enumerate() {
                vars[c + 1] = col[i];
            }
            *slot = self.eval(&vars);
        }
    }
}

/// Post-order lowering of an [`Expr`] into `tape`, returning the peak stack
/// depth reached by this subtree given it starts at height `sp`.
fn lower(expr: &Expr, tape: &mut Vec<Instr>, luts: &mut Vec<Lut>, sp: usize, peak: usize) -> usize {
    match expr {
        Expr::Const(c) => {
            tape.push(Instr::Const(*c));
            peak.max(sp + 1)
        }
        Expr::Var(i) => {
            tape.push(Instr::Var(*i));
            peak.max(sp + 1)
        }
        Expr::Unary(op, child) => {
            // Child leaves one value; the op replaces it in place.
            let peak = lower(child, tape, luts, sp, peak);
            tape.push(Instr::Unary(*op));
            peak
        }
        Expr::Binary(op, lhs, rhs) => {
            // lhs occupies slot `sp`; rhs is evaluated above it, so the
            // subtree's peak accounts for both operands being live at once.
            let peak = lower(lhs, tape, luts, sp, peak);
            let peak = lower(rhs, tape, luts, sp + 1, peak);
            tape.push(Instr::Binary(*op));
            peak
        }
        Expr::Curve(child, lut) => {
            // Same stack shape as a unary op; the table moves out-of-line and
            // the tape keeps its index. Identical tables are deduplicated —
            // one curve reused across a graph is one table.
            let peak = lower(child, tape, luts, sp, peak);
            let index = luts.iter().position(|l| l == lut).unwrap_or_else(|| {
                luts.push(lut.clone());
                luts.len() - 1
            });
            // A tape referencing more than u16::MAX distinct curves is not a
            // real program; saturating keeps `lower` total, and the eval-side
            // bounds check turns any such lookup into the identity.
            tape.push(Instr::Curve(u16::try_from(index).unwrap_or(u16::MAX)));
            peak
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::hint::black_box;
    use std::time::Instant;

    /// A canonical driver expression: `amp * sin(freq * t + phase)`, with
    /// amp/freq/phase as per-element columns (vars 1/2/3) over time (var 0).
    fn oscillator() -> Expr {
        Expr::binary(
            BinaryOp::Mul,
            Expr::var(1),
            Expr::unary(
                UnaryOp::Sin,
                Expr::binary(
                    BinaryOp::Add,
                    Expr::binary(BinaryOp::Mul, Expr::var(2), Expr::var(0)),
                    Expr::var(3),
                ),
            ),
        )
    }

    #[test]
    fn compiled_matches_reference_on_the_oscillator() {
        let expr = oscillator();
        let kernel = Kernel::compile(&expr).expect("compiles");
        for &t in &[0.0, 0.5, 1.0, 3.3, -2.0] {
            let vars = [t, 2.0, 1.5, 0.25];
            let got = kernel.eval(&vars);
            let want = expr.eval_ref(&vars);
            assert!((got - want).abs() < 1e-5, "t={t}: {got} vs {want}");
        }
    }

    #[test]
    fn drive_broadcasts_time_and_maps_columns() {
        let kernel = Kernel::compile(&oscillator()).expect("compiles");
        let amp = [1.0_f32, 2.0, 3.0];
        let freq = [1.0_f32, 1.0, 1.0];
        let phase = [0.0_f32, 0.0, 0.0];
        let mut out = [0.0_f32; 3];
        let t = std::f32::consts::FRAC_PI_2; // sin = 1
        kernel.drive(t, &[&amp, &freq, &phase], &mut out);
        for (o, a) in out.iter().zip(amp) {
            assert!((o - a).abs() < 1e-5, "{o} vs {a}");
        }
    }

    #[test]
    fn deep_expression_is_rejected_not_panicked() {
        // Left-nested adds keep only one extra operand live, so build a
        // right-heavy tree that forces stack growth past MAX_STACK.
        let mut e = Expr::var(0);
        for _ in 0..(MAX_STACK + 4) {
            e = Expr::binary(BinaryOp::Add, Expr::var(0), e);
        }
        assert!(matches!(
            Kernel::compile(&e),
            Err(KernelError::TooDeep { .. })
        ));
    }

    #[test]
    fn out_of_range_variable_is_rejected() {
        let e = Expr::var(MAX_VARS as u8);
        assert!(matches!(
            Kernel::compile(&e),
            Err(KernelError::VarOutOfRange { .. })
        ));
    }

    // A small recursive strategy that builds random valid expressions.
    fn arb_expr() -> impl Strategy<Value = Expr> {
        let leaf = prop_oneof![
            (-10.0_f32..10.0).prop_map(Expr::Const),
            (0u8..4).prop_map(Expr::Var),
        ];
        leaf.prop_recursive(6, 64, 2, |inner| {
            prop_oneof![
                (
                    prop_oneof![
                        Just(UnaryOp::Neg),
                        Just(UnaryOp::Sin),
                        Just(UnaryOp::Cos),
                        Just(UnaryOp::Sqrt),
                        Just(UnaryOp::Abs),
                    ],
                    inner.clone()
                )
                    .prop_map(|(op, c)| Expr::unary(op, c)),
                (
                    prop_oneof![
                        Just(BinaryOp::Add),
                        Just(BinaryOp::Sub),
                        Just(BinaryOp::Mul),
                        Just(BinaryOp::Min),
                        Just(BinaryOp::Max),
                    ],
                    inner.clone(),
                    inner.clone()
                )
                    .prop_map(|(op, l, r)| Expr::binary(op, l, r)),
                // A curve node, so the tape's out-of-line table handling is
                // covered by the same oracle as everything else.
                (prop::collection::vec(-2.0_f32..2.0, 2..8), inner).prop_map(|(samples, c)| {
                    Expr::Curve(Box::new(c), Lut::from_samples(&samples))
                }),
            ]
        })
    }

    #[test]
    fn a_lut_lerps_between_its_samples_and_clamps_outside() {
        let lut = Lut::from_samples(&[0.0, 1.0, 0.0]);
        assert!((lut.lookup(0.0) - 0.0).abs() < 1e-6);
        assert!((lut.lookup(0.5) - 1.0).abs() < 1e-6, "the middle sample");
        assert!((lut.lookup(0.25) - 0.5).abs() < 1e-6, "lerped halfway");
        assert!((lut.lookup(1.0) - 0.0).abs() < 1e-6);
        // Outside [0, 1] holds the endpoints rather than extrapolating.
        assert!((lut.lookup(-5.0) - 0.0).abs() < 1e-6);
        assert!((lut.lookup(5.0) - 0.0).abs() < 1e-6);
    }

    /// A table shorter than two entries would make `lookup`'s lerp index out
    /// of bounds, so construction pads instead of trusting the caller.
    #[test]
    fn a_degenerate_lut_is_padded_not_rejected() {
        assert_eq!(Lut::from_samples(&[]).samples().len(), 2);
        let one = Lut::from_samples(&[7.0]);
        assert_eq!(one.samples().len(), 2);
        assert!((one.lookup(0.3) - 7.0).abs() < 1e-6, "a constant table");
    }

    #[test]
    fn sampling_a_function_reproduces_it_at_the_sample_points() {
        let lut = Lut::sample(8, |x| x * x);
        assert_eq!(lut.samples().len(), 9);
        assert!((lut.lookup(0.5) - 0.25).abs() < 1e-6);
        assert!((lut.lookup(1.0) - 1.0).abs() < 1e-6);
        // Resolution 0 would leave a one-entry table; it is raised to 1.
        assert_eq!(Lut::sample(0, |_| 1.0).samples().len(), 2);
    }

    /// The whole point of moving tables out-of-line: reusing one curve in
    /// several places costs one table, not several.
    #[test]
    fn identical_curves_share_one_table() {
        let lut = Lut::from_samples(&[0.0, 1.0]);
        let expr = Expr::binary(
            BinaryOp::Add,
            Expr::Curve(Box::new(Expr::var(0)), lut.clone()),
            Expr::Curve(Box::new(Expr::var(1)), lut),
        );
        let kernel = Kernel::compile(&expr).expect("compiles");
        assert_eq!(kernel.luts.len(), 1, "deduplicated");
        // …and it still evaluates both operands through it.
        assert!((kernel.eval(&[0.5, 1.0]) - 1.5).abs() < 1e-6);
    }

    proptest! {
        /// The compiled tape must agree with the tree-walking oracle for any
        /// expression and any inputs (barring NaN, where bit-equality is
        /// meaningless).
        #[test]
        fn compiled_kernel_matches_reference(
            expr in arb_expr(),
            vars in prop::array::uniform4(-20.0_f32..20.0),
        ) {
            let kernel = Kernel::compile(&expr).expect("bounded strategy compiles");
            let got = kernel.eval(&vars);
            let want = expr.eval_ref(&vars);
            prop_assert!(
                (got - want).abs() < 1e-3 || (got.is_nan() && want.is_nan()),
                "{got} vs {want} for {expr:?}"
            );
        }
    }

    /// Not an assertion of a hard budget (timing asserts are CI-flaky) — this
    /// runs the hot path over a particle-scale population and *prints* the
    /// throughput so the spike's central number is observable with
    /// `cargo test -- --nocapture`. The only assertions are correctness
    /// (finite, non-trivial) and a very generous ceiling to catch a
    /// catastrophic regression.
    #[test]
    fn throughput_at_particle_scale() {
        const N: usize = 1_000_000;
        let kernel = Kernel::compile(&oscillator()).expect("compiles");

        // Structure-of-arrays population, filled deterministically.
        let amp: Vec<f32> = (0..N).map(|i| 1.0 + (i % 7) as f32).collect();
        let freq: Vec<f32> = (0..N).map(|i| 0.5 + (i % 3) as f32).collect();
        let phase: Vec<f32> = (0..N).map(|i| (i % 5) as f32 * 0.1).collect();
        let mut out = vec![0.0_f32; N];

        // Warm caches / branch predictor.
        kernel.drive(0.0, &[&amp, &freq, &phase], &mut out);

        let frames = 20;
        let start = Instant::now();
        for f in 0..frames {
            let t = black_box(f as f32 * 0.016);
            kernel.drive(t, black_box(&[&amp, &freq, &phase]), &mut out);
            black_box(out[N - 1]);
        }
        let elapsed = start.elapsed();

        let evals = (N * frames) as f64;
        let per_sec = evals / elapsed.as_secs_f64();
        let per_frame_us = elapsed.as_secs_f64() * 1e6 / frames as f64;
        eprintln!(
            "kernel spike: {N} elems x {frames} frames in {:?} \
             => {:.1} M evals/s, {:.0} us/frame ({} instrs, depth {})",
            elapsed,
            per_sec / 1e6,
            per_frame_us,
            kernel.len(),
            kernel.stack_depth(),
        );

        assert!(out.iter().all(|v| v.is_finite()));
        // Catastrophic-regression ceiling only (debug, opt-level 1): 1M
        // evaluated 20 times in under 5s is a floor no sane build misses.
        assert!(
            elapsed.as_secs_f64() < 5.0,
            "hot path unexpectedly slow: {elapsed:?}"
        );
    }
}
