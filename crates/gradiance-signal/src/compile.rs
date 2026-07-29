//! Lowering the authored, by-name [`SignalExpr`] to the pure Tier-B
//! [`Kernel`] (`gradiance_kernel`) — the P2 "modulator lowers to a
//! compiled, allocation-free numeric kernel" rule in action
//! (`docs/signal-dataflow.md`, `CLAUDE.md`).
//!
//! A computed signal is authored over named inputs (bus signals, params,
//! `"t"`). Here we resolve those names to the kernel's positional variable
//! slots once (cold path) and compile the tape; the per-frame evaluator
//! then gathers the named values into a `vars` array and calls
//! [`Kernel::eval`] with zero allocation.

use gradiance_domain::signal::SignalExpr;
use gradiance_kernel::{BinaryOp, Expr, Kernel, KernelError, Lut, UnaryOp};

/// A computed signal compiled for the hot path: the ordered input names to
/// gather (var 0, 1, …) plus the tape that consumes them.
#[derive(Debug, Clone)]
pub struct CompiledSignal {
    /// Bus names to read, in the order the kernel's variables expect them.
    pub inputs: Vec<String>,
    /// The compiled evaluation tape.
    pub kernel: Kernel,
}

/// Lowers an authored expression to a [`CompiledSignal`]. Fails exactly
/// when the kernel does (too deep, or more than `MAX_VARS` distinct
/// inputs) — surfaced to the UI rather than silently dropped.
pub fn compile(expr: &SignalExpr) -> Result<CompiledSignal, KernelError> {
    let inputs = expr.inputs();
    let kernel = Kernel::compile(&lower(expr, &inputs))?;
    Ok(CompiledSignal { inputs, kernel })
}

/// Recursively rewrites named inputs to positional [`Expr::Var`] reads.
/// An input not in `inputs` cannot occur (the list came from this same
/// tree), but a missing name degrades to `Var(0)` rather than panicking.
fn lower(expr: &SignalExpr, inputs: &[String]) -> Expr {
    let idx = |name: &str| inputs.iter().position(|n| n == name).unwrap_or(0) as u8;
    match expr {
        SignalExpr::Const(c) => Expr::constant(*c),
        SignalExpr::Input(name) => Expr::var(idx(name)),
        SignalExpr::Neg(a) => Expr::unary(UnaryOp::Neg, lower(a, inputs)),
        SignalExpr::Sin(a) => Expr::unary(UnaryOp::Sin, lower(a, inputs)),
        SignalExpr::Cos(a) => Expr::unary(UnaryOp::Cos, lower(a, inputs)),
        SignalExpr::Abs(a) => Expr::unary(UnaryOp::Abs, lower(a, inputs)),
        SignalExpr::Add(a, b) => Expr::binary(BinaryOp::Add, lower(a, inputs), lower(b, inputs)),
        SignalExpr::Sub(a, b) => Expr::binary(BinaryOp::Sub, lower(a, inputs), lower(b, inputs)),
        SignalExpr::Mul(a, b) => Expr::binary(BinaryOp::Mul, lower(a, inputs), lower(b, inputs)),
        SignalExpr::Div(a, b) => Expr::binary(BinaryOp::Div, lower(a, inputs), lower(b, inputs)),
        SignalExpr::Min(a, b) => Expr::binary(BinaryOp::Min, lower(a, inputs), lower(b, inputs)),
        SignalExpr::Max(a, b) => Expr::binary(BinaryOp::Max, lower(a, inputs), lower(b, inputs)),
        // The curve is *sampled here* — at compile time, on the cold path —
        // into the kernel's uniform table. That is the whole two-tier rule in
        // one line: the authoring shape (control points, monotone-cubic
        // interpolation, a binary search per lookup) never reaches the frame
        // loop; what reaches it is a clamp and a lerp.
        SignalExpr::Curve(a, curve) => Expr::Curve(
            Box::new(lower(a, inputs)),
            Lut::sample(CURVE_LUT_RESOLUTION, |x| curve.eval(x)),
        ),
    }
}

/// Intervals in a lowered curve's lookup table.
///
/// 128 puts the sampling error of a monotone cubic well below the precision
/// any of these signals drive (a colour channel, a tracer width), while the
/// table stays 516 bytes — small enough to sit in cache alongside the tape.
const CURVE_LUT_RESOLUTION: usize = 128;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_and_evaluates_over_named_inputs() {
        // 2 * sin(t) + speed
        let expr = SignalExpr::Add(
            Box::new(SignalExpr::Mul(
                Box::new(SignalExpr::Const(2.0)),
                Box::new(SignalExpr::Sin(Box::new(SignalExpr::Input("t".into())))),
            )),
            Box::new(SignalExpr::Input("speed".into())),
        );
        let compiled = compile(&expr).expect("compiles");
        assert_eq!(compiled.inputs, vec!["t".to_string(), "speed".to_string()]);
        // t = π/2 (sin = 1), speed = 10 → 2*1 + 10 = 12.
        let vars = [std::f32::consts::FRAC_PI_2, 10.0];
        assert!((compiled.kernel.eval(&vars) - 12.0).abs() < 1e-5);
    }

    #[test]
    fn too_many_inputs_is_rejected() {
        // More than MAX_VARS distinct inputs cannot compile.
        let mut expr = SignalExpr::Input("i0".into());
        for i in 1..20 {
            expr = SignalExpr::Add(Box::new(expr), Box::new(SignalExpr::Input(format!("i{i}"))));
        }
        assert!(compile(&expr).is_err());
    }
}
