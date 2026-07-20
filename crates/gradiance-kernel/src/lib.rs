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
        }
    }

    /// Highest variable index referenced, if any (used to size inputs).
    pub fn max_var(&self) -> Option<u8> {
        match self {
            Self::Const(_) => None,
            Self::Var(i) => Some(*i),
            Self::Unary(_, child) => child.max_var(),
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
}

/// A compiled driver expression: a flat tape plus its proven stack depth.
///
/// Built once by [`compile`](Kernel::compile) (Tier A / cold path), then
/// evaluated many times by [`eval`](Kernel::eval) / [`drive`](Kernel::drive)
/// (Tier B / hot path) with no allocation.
#[derive(Debug, Clone)]
pub struct Kernel {
    tape: Vec<Instr>,
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
        let depth = lower(expr, &mut tape, 0, 0);
        if depth > MAX_STACK {
            return Err(KernelError::TooDeep { needed: depth });
        }
        Ok(Self {
            tape,
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
fn lower(expr: &Expr, tape: &mut Vec<Instr>, sp: usize, peak: usize) -> usize {
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
            let peak = lower(child, tape, sp, peak);
            tape.push(Instr::Unary(*op));
            peak
        }
        Expr::Binary(op, lhs, rhs) => {
            // lhs occupies slot `sp`; rhs is evaluated above it, so the
            // subtree's peak accounts for both operands being live at once.
            let peak = lower(lhs, tape, sp, peak);
            let peak = lower(rhs, tape, sp + 1, peak);
            tape.push(Instr::Binary(*op));
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
                    inner
                )
                    .prop_map(|(op, l, r)| Expr::binary(op, l, r)),
            ]
        })
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
