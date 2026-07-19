//! Large-deflection elastica reduction — the flexure rod's force law.
//!
//! A planar inextensible beam ("elastica") between two end frames,
//! approximated by a **two-mode Rayleigh–Ritz energy reduction** instead
//! of the exact elliptic-integral boundary-value problem. The reduction
//! keeps the elastica's signature behavior — the Euler pitchfork
//! (buckling at exactly `P_cr = π²EI/L²`), post-buckling bowing whose
//! axial force plateaus at `P_cr`, snap-through under end rotation —
//! while staying closed-form, deterministic, and a few hundred flops per
//! rod per frame. The public seam ([`solve`] / [`centerline`]) is
//! backend-agnostic so an exact elliptic-integral solver can replace the
//! internals later without touching any ECS wiring.
//!
//! # Model
//!
//! Work in the corotated **chord frame**: x̂ from anchor A to anchor B,
//! chord length `c`, beam arclength `L`. With `ξ = s/L`, the transverse
//! centerline is
//!
//! ```text
//! w(s) = φa·g₁(s) + φb·g₂(s) + q₁·m₁(ξ) + q₂·m₂(ξ)
//! g₁ = s(1−ξ)²      g₂ = s·ξ(ξ−1)          (unit-end-slope Hermite cubics)
//! m₁ = sin(πξ) − πξ(1−ξ)                    (slope-free internal modes:
//! m₂ = sin(2πξ) − 2πξ(1−ξ)(1−2ξ)            m(0)=m(1)=0, m′(0)=m′(1)=0)
//! ```
//!
//! so `φa`/`φb` are the **true** end slopes (`w′(0)=φa`, `w′(L)=φb`) and
//! the internal modes never leak slope into a welded end. Because the
//! cubics satisfy `w⁗=0` and the m-modes vanish with their slopes at the
//! ends, the bending matrix is block-diagonal — the linear weld-beam
//! moments (`4EI/L`, `2EI/L`) are reproduced exactly. Over the DOF
//! vector `x = (φa, φb, q₁, q₂)`:
//!
//! - bending energy `U_b = ½·xᵀK x`, `K_ij = EI ∫ fᵢ″fⱼ″`;
//! - transverse arclength absorption `Δ = ½·xᵀG x`, `G_ij = ∫ fᵢ′fⱼ′`
//!   (the moderate-rotation inextensibility measure);
//! - axial energy `U_a = ½·k_ax·e²` with strain `e = c − L + Δ` and
//!   `k_ax = EA/L`.
//!
//! The quartic `k_ax·Δ²` coupling is what produces buckling: compressing
//! the chord past onset makes the straight state a saddle and the mode
//! amplitudes grow to absorb the excess length. With free (hinged) end
//! slopes the span contains `sin(πξ)` — the exact hinged buckling
//! eigenfunction — so the pitchfork threshold is exactly `π²EI/L²`
//! (asserted in tests).
//!
//! Fixed ends prescribe their `φ` from the attached body's rotation;
//! hinge ends leave `φ` as an internal minimized DOF (moment-free by
//! construction). Free DOFs are minimized each call by a fixed-budget
//! damped-Newton iteration warm-started from the previous solution (the
//! warm start is also what tracks the buckling branch; a fixed tiny seed
//! breaks the symmetric tie deterministically).
//!
//! End loads are energy gradients — see [`ElasticaLoads`]. Because the
//! energy is invariant under rigid motions of the pair, the returned
//! wrench pair conserves linear *and* angular momentum when applied
//! equal-and-opposite **at the anchor points**.

use bevy::math::Vec2;

/// Newton iterations per [`solve`] call (fixed budget → deterministic).
const NEWTON_ITERS: usize = 4;
/// Explicit-integration stability clamp `k_eff ≤ ALPHA·m/h²`.
const ALPHA: f32 = 0.25;
/// Symmetry-breaking seed for the first buckling mode, relative to `L`.
const SEED: f32 = 1.0e-4;

/// The rod's material/geometry constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElasticaParams {
    /// Beam arclength (px); the chord equals it at rest.
    pub length: f32,
    /// Axial rigidity `EA` (force).
    pub ea: f32,
    /// Bending rigidity `EI` (force·px²).
    pub ei: f32,
    /// End A is a hinge (moment-free; its tangent is solved, not prescribed).
    pub hinge_a: bool,
    /// End B likewise.
    pub hinge_b: bool,
}

/// One frame's kinematic inputs, all in the chord frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElasticaInput {
    /// Material/geometry constants.
    pub params: ElasticaParams,
    /// Current chord length `c = |pB − pA|` (px).
    pub chord: f32,
    /// End A tangent angle relative to the chord (rad; ignored if hinged).
    pub phi_a: f32,
    /// End B likewise.
    pub phi_b: f32,
    /// Chord-length rate `ċ` (px/s) — relative anchor velocity along x̂.
    pub dc: f32,
    /// Relative anchor velocity across the chord (px/s) — `c·ψ̇`.
    pub v_t: f32,
    /// End A tangent rate `φ̇a = ω_A − ψ̇` (rad/s).
    pub dphi_a: f32,
    /// End B likewise.
    pub dphi_b: f32,
    /// Stiffness-proportional damping ratio ζ.
    pub zeta: f32,
    /// Reduced translational mass of the pair (`1/(1/mA + 1/mB)`).
    pub m_red: f32,
    /// Reduced rotational inertia of the pair.
    pub i_red: f32,
    /// The explicit-force hold interval (s): the *fixed step* dt, not the
    /// substep — the accumulated force is computed once per frame and held
    /// constant across the whole physics step, so that is the stability
    /// timescale.
    pub h_step: f32,
}

/// Warm-start cache: the last converged DOF vector `(φa, φb, q₁, q₂)`.
///
/// Deterministic — it evolves purely from inputs; a cold start (zeros)
/// re-converges within a few frames.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ElasticaState {
    /// Last solution `(φa, φb, q₁, q₂)`.
    pub x: [f32; 4],
}

/// End loads in the chord frame, damping included.
///
/// Apply to body B **at its anchor point**: force
/// `R(ψ)·force_b`, torque `moment_b`. Apply the exact negation of the
/// force to body A at *its* anchor, plus torque `moment_a`. The pair has
/// zero net force and zero net moment about any point.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ElasticaLoads {
    /// Force on B in the chord frame (x̂ = A→B).
    pub force_b: Vec2,
    /// Torque on body A (moment-free ends report 0).
    pub moment_a: f32,
    /// Torque on body B.
    pub moment_b: f32,
}

/// Bending stiffness matrix `K` for the ansatz basis (symmetric 4×4).
fn k_matrix(ei: f32, l: f32) -> [[f32; 4]; 4] {
    use std::f32::consts::PI;
    let l3 = l * l * l;
    let k11 = 4.0 * ei / l;
    let k12 = 2.0 * ei / l;
    // Slope/internal coupling vanishes by Galerkin orthogonality: the
    // cubics have w⁗ = 0 and the m-modes vanish (with slope) at the ends.
    let k33 = (PI.powi(4) / 2.0 - 4.0 * PI * PI) * ei / l3;
    let k44 = (8.0 * PI.powi(4) - 48.0 * PI * PI) * ei / l3;
    [
        [k11, k12, 0.0, 0.0],
        [k12, k11, 0.0, 0.0],
        [0.0, 0.0, k33, 0.0],
        [0.0, 0.0, 0.0, k44],
    ]
}

/// Geometric (inextensibility) matrix `G` (symmetric 4×4): `Δ = ½ xᵀG x`.
fn g_matrix(l: f32) -> [[f32; 4]; 4] {
    use std::f32::consts::PI;
    let g11 = 2.0 * l / 15.0;
    let g12 = -l / 30.0;
    let g13 = 2.0 / PI - PI / 6.0;
    let g14 = 3.0 / PI - PI / 5.0;
    let g23 = -g13;
    let g24 = g14;
    let g33 = (5.0 * PI * PI / 6.0 - 8.0) / l;
    let g44 = (14.0 * PI * PI / 5.0 - 24.0) / l;
    [
        [g11, g12, g13, g14],
        [g12, g11, g23, g24],
        [g13, g23, g33, 0.0],
        [g14, g24, 0.0, g44],
    ]
}

fn mat_vec(m: &[[f32; 4]; 4], x: &[f32; 4]) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (i, row) in m.iter().enumerate() {
        out[i] = row[0] * x[0] + row[1] * x[1] + row[2] * x[2] + row[3] * x[3];
    }
    out
}

fn dot(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

/// Axial strain `e = c − L + ½ xᵀG x`.
fn strain(x: &[f32; 4], g: &[[f32; 4]; 4], c: f32, l: f32) -> f32 {
    c - l + 0.5 * dot(x, &mat_vec(g, x))
}

/// Full energy gradient `∇U = K x + k_ax·e·G x`.
fn gradient(x: &[f32; 4], k: &[[f32; 4]; 4], g: &[[f32; 4]; 4], k_ax: f32, e: f32) -> [f32; 4] {
    let kx = mat_vec(k, x);
    let gx = mat_vec(g, x);
    [
        kx[0] + k_ax * e * gx[0],
        kx[1] + k_ax * e * gx[1],
        kx[2] + k_ax * e * gx[2],
        kx[3] + k_ax * e * gx[3],
    ]
}

/// Whether the leading n×n block is positive definite (Cholesky test).
fn is_positive_definite(m: &[[f32; 4]; 4], n: usize) -> bool {
    let mut l = [[0.0f32; 4]; 4];
    for i in 0..n {
        for j in 0..=i {
            let mut sum = m[i][j];
            sum -= l[i][..j]
                .iter()
                .zip(&l[j][..j])
                .map(|(a, b)| a * b)
                .sum::<f32>();
            if i == j {
                if sum <= 0.0 {
                    return false;
                }
                l[i][j] = sum.sqrt();
            } else {
                l[i][j] = sum / l[j][j];
            }
        }
    }
    true
}

/// Solves the free-subspace Newton step `H δ = −g` (n ≤ 4) by Gaussian
/// elimination with partial pivoting; `None` if singular.
fn solve_linear(h: &mut [[f32; 4]; 4], rhs: &mut [f32; 4], n: usize) -> Option<[f32; 4]> {
    for col in 0..n {
        let pivot = (col..n).max_by(|&a, &b| h[a][col].abs().total_cmp(&h[b][col].abs()))?;
        if h[pivot][col].abs() < 1.0e-12 {
            return None;
        }
        h.swap(col, pivot);
        rhs.swap(col, pivot);
        for row in (col + 1)..n {
            let f = h[row][col] / h[col][col];
            let pivot_row = h[col];
            for (c2, entry) in h[row].iter_mut().enumerate().take(n).skip(col) {
                *entry -= f * pivot_row[c2];
            }
            rhs[row] -= f * rhs[col];
        }
    }
    let mut out = [0.0; 4];
    for row in (0..n).rev() {
        let mut sum = rhs[row];
        for (c2, &val) in out.iter().enumerate().take(n).skip(row + 1) {
            sum -= h[row][c2] * val;
        }
        out[row] = sum / h[row][row];
    }
    Some(out)
}

/// Minimizes the energy over the free DOFs (fixed Newton budget with
/// Levenberg damping), updating `x` in place.
fn minimize(
    x: &mut [f32; 4],
    free: [bool; 4],
    k: &[[f32; 4]; 4],
    g: &[[f32; 4]; 4],
    k_ax: f32,
    c: f32,
    l: f32,
) {
    let free_idx: Vec<usize> = (0..4).filter(|&i| free[i]).collect();
    let n = free_idx.len();
    if n == 0 {
        return;
    }
    // Deterministic symmetry-breaking: nudge the first mode off the exact
    // pitchfork so compression buckles instead of balancing forever.
    if x[2].abs() < SEED * l {
        let bias = x[0] + x[1];
        x[2] = SEED * l * if bias < 0.0 { -1.0 } else { 1.0 };
    }
    // Newton runs in non-dimensionalized DOF space (mode amplitudes in
    // units of L, slopes in radians) so every direction has a comparable
    // curvature scale — a uniform Levenberg shift then damps them evenly
    // instead of freezing the soft (buckling) direction to protect the
    // stiff ones.
    let dof_scale = [1.0, 1.0, l, l];
    // Scaled step cap ≈ half a radian / half a length per iteration.
    let step_cap = 0.5;
    for _ in 0..NEWTON_ITERS {
        let e = strain(x, g, c, l);
        let grad = gradient(x, k, g, k_ax, e);
        let gx = mat_vec(g, x);
        // Free-subspace Hessian: K + k_ax (Gx)(Gx)ᵀ + k_ax e G, scaled
        // into non-dimensional space (H' = D·H·D, g' = D·g).
        let mut h = [[0.0f32; 4]; 4];
        let mut rhs = [0.0f32; 4];
        let mut scale = 0.0f32;
        for (r, &i) in free_idx.iter().enumerate() {
            for (s, &j) in free_idx.iter().enumerate() {
                h[r][s] =
                    dof_scale[i] * (k[i][j] + k_ax * (gx[i] * gx[j] + e * g[i][j])) * dof_scale[j];
            }
            scale = scale.max(h[r][r].abs());
            rhs[r] = -grad[i] * dof_scale[i];
        }
        // Positive-definite-ified Newton: plain Newton converges to the
        // *nearest stationary point*, which at a compressed straight rod
        // is the saddle — the buckled minima would never be found. Raise
        // the Levenberg shift λ (deterministic ×10 schedule) until the
        // shifted Hessian is PD, guaranteeing a descent direction.
        let mut delta = None;
        // Gentle ×3 escalation: a shift that barely clears the most
        // negative eigenvalue keeps the descent step along the buckling
        // direction large (fast pitchfork escape); ×10 would overshoot
        // and crawl.
        let mut lambda = 1.0e-6 * scale.max(1.0e-6);
        for _ in 0..16 {
            let mut shifted = h;
            for (r, row) in shifted.iter_mut().enumerate().take(n) {
                row[r] += lambda;
            }
            if is_positive_definite(&shifted, n) {
                let mut rhs_try = rhs;
                delta = solve_linear(&mut shifted, &mut rhs_try, n);
                break;
            }
            lambda *= 3.0;
        }
        let Some(delta) = delta else {
            break;
        };
        let mut bad = false;
        for (r, &i) in free_idx.iter().enumerate() {
            // Map the scaled step back to real units (δ = D·δ').
            let step = delta[r].clamp(-step_cap, step_cap) * dof_scale[i];
            if !step.is_finite() {
                bad = true;
                break;
            }
            x[i] += step;
        }
        if bad {
            // Non-finite state: restart from straight next frame.
            *x = [0.0; 4];
            break;
        }
    }
}

/// The stability-clamped effective rigidities `(ea, ei)` for this step.
///
/// Explicit forces held over a substep `h` are stable only up to
/// `k ≤ α·m/h²`; scaling `EA`/`EI` uniformly keeps every stiffness block
/// positive semi-definite and the couplings consistent.
fn clamped_rigidity(input: &ElasticaInput) -> (f32, f32) {
    let p = &input.params;
    let l = p.length.max(1.0e-3);
    let h2 = (input.h_step * input.h_step).max(1.0e-12);
    let m = input.m_red.max(1.0e-6);
    let i_r = input.i_red.max(1.0e-6);
    let cap = |k: f32, inertia: f32| {
        if k <= 0.0 {
            1.0
        } else {
            (ALPHA * inertia / (h2 * k)).min(1.0)
        }
    };
    let s_ax = cap(p.ea / l, m);
    // Bending's stiffest translational and rotational entries.
    let s_bend = cap(12.0 * p.ei / (l * l * l), m).min(cap(4.0 * p.ei / l, i_r));
    (p.ea * s_ax, p.ei * s_bend)
}

/// Solves one frame: minimizes the internal DOFs (warm-started from
/// `state`) and returns the end loads, damping included.
pub fn solve(input: &ElasticaInput, state: &mut ElasticaState) -> ElasticaLoads {
    let p = &input.params;
    let l = p.length.max(1.0e-3);
    let (ea, ei) = clamped_rigidity(input);
    let k_ax = ea / l;
    let k = k_matrix(ei, l);
    let g = g_matrix(l);

    // Prescribed tangents come from the bodies; hinged ones stay solved.
    if !p.hinge_a {
        state.x[0] = input.phi_a;
    }
    if !p.hinge_b {
        state.x[1] = input.phi_b;
    }
    if state.x.iter().any(|v| !v.is_finite()) {
        state.x = [0.0; 4];
    }
    let free = [p.hinge_a, p.hinge_b, true, true];
    minimize(&mut state.x, free, &k, &g, k_ax, input.chord, l);

    let x = &state.x;
    let e = strain(x, &g, input.chord, l);
    let grad = gradient(x, &k, &g, k_ax, e);
    // ga/gb vanish at hinged ends by construction (they were minimized).
    let (ga, gb) = (grad[0], grad[1]);

    // Damping coefficients: stiffness-proportional, per generalized DOF,
    // against the clamped stiffnesses so they obey the same step bound.
    let zeta = input.zeta.max(0.0);
    let c_ax = 2.0 * zeta * (k_ax * input.m_red.max(0.0)).sqrt();
    let k_tr = 12.0 * ei / (l * l * l);
    let c_tr = 2.0 * zeta * (k_tr * input.m_red.max(0.0)).sqrt();
    let k_rot = 4.0 * ei / l;
    let c_rot = 2.0 * zeta * (k_rot * input.i_red.max(0.0)).sqrt();

    // Loads on B in the chord frame (equal-and-opposite pair at the two
    // anchors; see the module docs for the momentum bookkeeping).
    let c_len = input.chord.max(1.0e-3);
    let f_x = -k_ax * e - c_ax * input.dc;
    let f_y = (ga + gb) / c_len - c_tr * input.v_t;
    let moment_a = if p.hinge_a {
        0.0
    } else {
        -ga - c_rot * input.dphi_a
    };
    let moment_b = if p.hinge_b {
        0.0
    } else {
        -gb - c_rot * input.dphi_b
    };
    ElasticaLoads {
        force_b: Vec2::new(f_x, f_y),
        moment_a,
        moment_b,
    }
}

/// Samples the deflected centerline in the chord frame: `samples` points
/// from anchor A `(0, 0)` to anchor B `(chord, 0)`, transverse deflection
/// from the current state. Rotate by the chord angle and translate by
/// anchor A to draw in world space.
pub fn centerline(
    params: &ElasticaParams,
    state: &ElasticaState,
    chord: f32,
    samples: usize,
) -> Vec<Vec2> {
    use std::f32::consts::PI;
    let l = params.length.max(1.0e-3);
    let n = samples.max(2);
    let x = &state.x;
    (0..n)
        .map(|i| {
            let xi = i as f32 / (n - 1) as f32;
            let s = xi * l;
            let g1 = s * (1.0 - xi) * (1.0 - xi);
            let g2 = s * xi * (xi - 1.0);
            let m1 = (PI * xi).sin() - PI * xi * (1.0 - xi);
            let m2 = (2.0 * PI * xi).sin() - 2.0 * PI * xi * (1.0 - xi) * (1.0 - 2.0 * xi);
            let w = x[0] * g1 + x[1] * g2 + x[2] * m1 + x[3] * m2;
            Vec2::new(xi * chord, w)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn params(hinge_a: bool, hinge_b: bool) -> ElasticaParams {
        ElasticaParams {
            length: 100.0,
            ea: 5.0e5,
            ei: 1.0e6,
            hinge_a,
            hinge_b,
        }
    }

    /// Static input (no rates, no damping), huge inertia so the clamp
    /// never bites in the analytic checks.
    fn static_input(p: ElasticaParams, chord: f32, phi_a: f32, phi_b: f32) -> ElasticaInput {
        ElasticaInput {
            params: p,
            chord,
            phi_a,
            phi_b,
            dc: 0.0,
            v_t: 0.0,
            dphi_a: 0.0,
            dphi_b: 0.0,
            zeta: 0.0,
            m_red: 1.0e12,
            i_red: 1.0e12,
            h_step: 1.0 / 60.0,
        }
    }

    /// Iterates `solve` until the warm-started state settles.
    fn converge(input: &ElasticaInput) -> (ElasticaState, ElasticaLoads) {
        let mut state = ElasticaState::default();
        let mut loads = ElasticaLoads::default();
        for _ in 0..200 {
            loads = solve(input, &mut state);
        }
        (state, loads)
    }

    #[test]
    fn tension_is_straight_and_axial_only() {
        let p = params(true, true);
        let (state, loads) = converge(&static_input(p, 101.0, 0.0, 0.0));
        // Stretched by 1 px: pure axial pull back toward A, no bowing.
        let expect = p.ea / p.length; // k_ax · e, e = 1
        assert!((loads.force_b.x + expect).abs() / expect < 1.0e-3);
        assert!(loads.force_b.y.abs() < 1.0e-3 * expect);
        assert!(state.x[2].abs() < 0.05, "no buckle in tension");
    }

    #[test]
    fn small_end_rotation_matches_the_linear_beam() {
        // Weld-weld, tiny end rotation at B only, chord at rest length:
        // the linear beam gives M_b = 4EI/L·φ, M_a = 2EI/L·φ (plus a
        // small correction from the sine coupling; 5% tolerance).
        let p = params(false, false);
        let phi = 1.0e-3;
        let (_, loads) = converge(&static_input(p, p.length, 0.0, phi));
        let mb = 4.0 * p.ei / p.length * phi;
        let ma = 2.0 * p.ei / p.length * phi;
        assert!(
            (loads.moment_b + mb).abs() / mb < 0.05,
            "M_b {} vs −{mb}",
            loads.moment_b
        );
        assert!(
            (loads.moment_a + ma).abs() / ma < 0.25,
            "M_a {} vs −{ma}",
            loads.moment_a
        );
    }

    #[test]
    fn compression_buckles_at_the_euler_load() {
        let p = params(true, true);
        let l = p.length;
        let p_cr = PI * PI * p.ei / (l * l);
        // 2% chord compression — far past onset (onset shortening is
        // P_cr/k_ax = 0.197 px here).
        let c = 0.98 * l;
        let (state, loads) = converge(&static_input(p, c, 0.0, 0.0));
        // Post-buckling bow: the hinged elastica bows as a·sin(πs/L)
        // with a = (2/π)·√(L·(L−c)) from inextensibility.
        let bow = centerline(&p, &state, c, 65)
            .iter()
            .map(|v| v.y.abs())
            .fold(0.0f32, f32::max);
        let expect_bow = (2.0 / PI) * (l * (l - c)).sqrt();
        assert!(bow > 0.5, "buckled (bow = {bow})");
        assert!(
            (bow - expect_bow).abs() / expect_bow < 0.15,
            "bow {bow} vs {expect_bow}"
        );
        // Axial force plateaus at the Euler load (compression pushes B
        // away from A: +x̂).
        assert!(
            (loads.force_b.x - p_cr).abs() / p_cr < 0.15,
            "axial {} vs P_cr {p_cr}",
            loads.force_b.x
        );
    }

    #[test]
    fn gradient_matches_finite_differences() {
        let p = params(false, false);
        let l = p.length;
        let k = k_matrix(p.ei, l);
        let g = g_matrix(l);
        let k_ax = p.ea / l;
        let c = 0.99 * l;
        let x = [0.05, -0.03, 4.0, -1.5];
        let e = strain(&x, &g, c, l);
        let grad = gradient(&x, &k, &g, k_ax, e);
        let energy = |x: &[f32; 4]| {
            let e = strain(x, &g, c, l);
            0.5 * dot(x, &mat_vec(&k, x)) + 0.5 * k_ax * e * e
        };
        for i in 0..4 {
            let h = 1.0e-2;
            let mut hi = x;
            let mut lo = x;
            hi[i] += h;
            lo[i] -= h;
            let fd = (energy(&hi) - energy(&lo)) / (2.0 * h);
            let tol = 2.0e-2 * grad[i].abs().max(1.0);
            assert!(
                (grad[i] - fd).abs() < tol,
                "dof {i}: analytic {} vs fd {fd}",
                grad[i]
            );
        }
    }

    #[test]
    fn solve_is_deterministic() {
        let p = params(true, false);
        let input = static_input(p, 0.97 * p.length, 0.0, 0.4);
        let run = || {
            let mut state = ElasticaState::default();
            let mut out = Vec::new();
            for _ in 0..50 {
                let loads = solve(&input, &mut state);
                out.push((state.x, loads));
            }
            out
        };
        assert_eq!(run(), run(), "bit-identical across runs");
    }

    #[test]
    fn stability_clamp_bounds_forces_for_tiny_masses() {
        let mut input = static_input(params(false, false), 90.0, 0.5, -0.5);
        input.m_red = 0.01;
        input.i_red = 0.01;
        let (_, loads) = converge(&input);
        assert!(loads.force_b.is_finite());
        // With the clamp, k_eff ≤ α·m/h²; a 10-px strain can't exceed
        // that bound times the strain magnitude (plus bending's share —
        // order-of-magnitude sanity, not exactness).
        let bound = ALPHA * input.m_red / (input.h_step * input.h_step) * 20.0;
        assert!(
            loads.force_b.length() < bound,
            "clamped force {} < {bound}",
            loads.force_b.length()
        );
    }

    #[test]
    fn centerline_hits_endpoints_and_bows_when_buckled() {
        let p = params(true, true);
        let c = 0.98 * p.length;
        let (state, _) = converge(&static_input(p, c, 0.0, 0.0));
        let pts = centerline(&p, &state, c, 24);
        assert_eq!(pts.len(), 24);
        assert!(pts[0].length() < 1.0e-4);
        assert!((pts[23] - Vec2::new(c, 0.0)).length() < 1.0e-4);
        let max_bow = pts.iter().map(|v| v.y.abs()).fold(0.0f32, f32::max);
        assert!(max_bow > 1.0, "buckled rod draws a visible bow");
    }
}
