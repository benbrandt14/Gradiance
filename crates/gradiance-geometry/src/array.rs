//! Array repetition geometry: how far to step so copies land flush.
//!
//! The question this module answers is the one that makes an array tool feel
//! right: **how far along a direction must a whole selection move so that the
//! moved copy just touches the original without overlapping it?** Step by
//! that distance and a single block becomes a seamless wall; a two-block
//! stack becomes a seamless tower.
//!
//! # Why it is not just the bounding box
//!
//! Using the selection's extent along the drag axis is the obvious answer and
//! it is right only for a convex, axis-aligned selection. Two bodies arranged
//! in a staircase can interlock: stepping by the full bounding height leaves
//! a visible gap where the copy could have nested. The exact answer is the
//! smallest `t` for which the translated set no longer intersects the
//! original — a one-dimensional slice of the no-fit polygon, and cheap to
//! compute exactly for convex pieces.
//!
//! # The exact computation
//!
//! For convex `A`, `B` and unit direction `d`, `A + t·d` overlaps `B` exactly
//! when the projections onto *every* separating-axis candidate overlap.
//! Along axis `n`, translating shifts `A`'s projection by `t·(d·n)`, so the
//! overlap condition is a plain interval in `t`:
//!
//! ```text
//! t·(d·n) ∈ (b_min − a_max,  b_max − a_min)
//! ```
//!
//! Intersecting those intervals over all axes gives the (convex) set of `t`
//! for which the pair overlaps at all. Its supremum is the smallest step that
//! clears the pair — and because the set is an *interval*, clearing at that
//! step also clears at every larger step, which is what makes one pitch valid
//! for copy 2, 3, and so on rather than only the first.
//!
//! Taking the maximum over every ordered pair of pieces gives the pitch for
//! the whole selection.
//!
//! # Tapered arrays
//!
//! When each copy is a scaled version of the last, the pitch is no longer one
//! number — but it is still closed-form. Let `H` be the selection's outline
//! about its centre and `u` the per-copy scale (one ratio per frame axis), so
//! copy `k` occupies `u^k ⊙ H`. Clearing copy `k+1` from copy `k` along a
//! frame axis `d` needs
//!
//! ```text
//! t_k · (u^-k ⊙ d) clears H from u ⊙ H   ⇒   t_k = u_d^k · Q
//! ```
//!
//! where `Q = contact_pitch_between(u ⊙ H, H, d)` — a single extra
//! measurement — and `u_d` is the scale component along `d`. The step down the
//! array is therefore *geometric*, and the position of copy `k` is the partial
//! sum `Q · (1 − u_d^k)/(1 − u_d)`. Exact whenever `d` is one of the frame
//! axes the scale is expressed in, which is every case a handle drag produces.
//!
//! # The conservative part
//!
//! Pieces are convex hulls of each body's outline, so a body with a bite cut
//! out of it steps as though the bite were filled. Results are therefore
//! always collision-free and never tighter than the true optimum — the same
//! trade the packing solver makes, for the same reason (convex overlap has an
//! exact cheap answer; general overlap does not).

use bevy::math::Vec2;

use crate::sat::axes_of;

/// Numeric floor for "this axis is perpendicular to the step direction".
const PARALLEL_EPS: f32 = 1e-6;

/// The open interval of `t` for which `a + t·d` overlaps `b`, or `None` when
/// no translation along `d` ever makes them overlap.
fn overlap_interval(a: &[Vec2], b: &[Vec2], d: Vec2) -> Option<(f32, f32)> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let mut axes: Vec<Vec2> = Vec::with_capacity(a.len() + b.len());
    axes_of(a, &mut axes);
    axes_of(b, &mut axes);
    if axes.is_empty() {
        return None;
    }

    let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
    for n in axes {
        let (a_min, a_max) = project(a, n);
        let (b_min, b_max) = project(b, n);
        let k = d.dot(n);
        // How much `a` may shift along `n` and still overlap `b`.
        let (shift_lo, shift_hi) = (b_min - a_max, b_max - a_min);
        if k.abs() <= PARALLEL_EPS {
            // This axis does not move under the translation: either the pair
            // already overlaps on it (no constraint) or it never will (no
            // amount of stepping along `d` can bring them together).
            if !(shift_lo < 0.0 && 0.0 < shift_hi) {
                return None;
            }
            continue;
        }
        let (axis_lo, axis_hi) = if k > 0.0 {
            (shift_lo / k, shift_hi / k)
        } else {
            (shift_hi / k, shift_lo / k)
        };
        lo = lo.max(axis_lo);
        hi = hi.min(axis_hi);
        if lo >= hi {
            return None;
        }
    }
    (lo < hi).then_some((lo, hi))
}

/// Projects `poly` onto unit axis `n`.
fn project(poly: &[Vec2], n: Vec2) -> (f32, f32) {
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    for p in poly {
        let d = p.dot(n);
        min = min.min(d);
        max = max.max(d);
    }
    (min, max)
}

/// The smallest step along `direction` that moves the whole set of convex
/// `pieces` clear of itself — the flush-contact array pitch.
///
/// Returns `None` when `direction` is degenerate or there is nothing to
/// measure. A set that already clears itself at every step returns `0.0`,
/// which callers should treat as "no natural pitch here".
pub fn contact_pitch(pieces: &[Vec<Vec2>], direction: Vec2) -> Option<f32> {
    contact_pitch_between(pieces, pieces, direction)
}

/// The smallest step along `direction` that moves every piece of `moving`
/// clear of every piece of `fixed`.
///
/// The two-set form of [`contact_pitch`], which is the same question asked of
/// an array whose copies are not all the same size: the copy that is about to
/// be placed (`moving`, already scaled) has to clear the copy already there
/// (`fixed`). Passing the same set twice recovers the uniform case.
pub fn contact_pitch_between(
    moving: &[Vec<Vec2>],
    fixed: &[Vec<Vec2>],
    direction: Vec2,
) -> Option<f32> {
    let d = direction.normalize_or_zero();
    if d == Vec2::ZERO || moving.is_empty() || fixed.is_empty() {
        return None;
    }
    let mut pitch = 0.0_f32;
    for a in moving {
        for b in fixed {
            // Ordered pairs, including a piece against its own original: the
            // copy of piece `a` has to clear *every* fixed piece, and its own
            // original is usually the binding one.
            if let Some((_, hi)) = overlap_interval(a, b, d) {
                pitch = pitch.max(hi);
            }
        }
    }
    pitch.is_finite().then_some(pitch.max(0.0))
}

/// Every piece scaled by `factors` about `pivot`, along axes rotated by
/// `basis`.
///
/// The array tool's per-copy taper acts in the selection's own frame, so the
/// scaled outline a pitch measurement needs is not simply `p * factors`.
pub fn scale_pieces(
    pieces: &[Vec<Vec2>],
    factors: Vec2,
    pivot: Vec2,
    basis: f32,
) -> Vec<Vec<Vec2>> {
    pieces
        .iter()
        .map(|piece| {
            piece
                .iter()
                .map(|p| crate::scale::scale_point(*p, pivot, basis, factors))
                .collect()
        })
        .collect()
}

/// The flush pitch of the *first* gap of a tapered array: the step that clears
/// a copy scaled by `factors` from the unscaled original.
///
/// Every later gap is this number times `factors`' component along
/// `direction`, raised to the copy index — see the module docs. Falls back to
/// the untapered answer when the taper is inert.
pub fn tapered_contact_pitch(
    pieces: &[Vec<Vec2>],
    direction: Vec2,
    factors: Vec2,
    pivot: Vec2,
    basis: f32,
) -> Option<f32> {
    if (factors - Vec2::ONE).abs().max_element() <= 1e-6 {
        return contact_pitch(pieces, direction);
    }
    let scaled = scale_pieces(pieces, factors, pivot, basis);
    contact_pitch_between(&scaled, pieces, direction)
}

/// The partial sum `1 + r + r² + … + r^(k−1)` — how many pitches down the
/// array copy `k` sits when each step is `r` times the last.
///
/// The `r → 1` limit is `k`, taken directly rather than through the closed
/// form so an inert taper is exact rather than a `0/0`.
#[must_use]
pub fn geometric_span(ratio: f32, k: u32) -> f32 {
    let k = k as f32;
    if !ratio.is_finite() || (ratio - 1.0).abs() < 1e-5 {
        return k;
    }
    (1.0 - ratio.powf(k)) / (1.0 - ratio)
}

/// How many tapered copies of pitch `step` (shrinking by `ratio` each time)
/// fit within `distance`.
///
/// Solves `step · geometric_span(ratio, n) ≤ distance` for the largest `n`. A
/// converging taper (`ratio < 1`) has a finite reach however far you drag —
/// `distance` beyond it yields `cap` rather than an unbounded count.
#[must_use]
pub fn copies_within(distance: f32, step: f32, ratio: f32, cap: u32) -> u32 {
    if distance <= 0.0 || step <= 0.0 || !distance.is_finite() || !step.is_finite() {
        return 0;
    }
    if !ratio.is_finite() || (ratio - 1.0).abs() < 1e-5 {
        return ((distance / step).floor() as i64).clamp(0, i64::from(cap)) as u32;
    }
    // span(n) = (1 - r^n)/(1 - r) ≤ distance/step  ⇒  r^n ≥ 1 - (1-r)·d/s.
    let bound = 1.0 - (1.0 - ratio) * distance / step;
    if bound <= 0.0 {
        // The array outruns the drag before it stops fitting.
        return cap;
    }
    let n = bound.ln() / ratio.ln();
    if !n.is_finite() {
        return cap;
    }
    (n.floor() as i64).clamp(0, i64::from(cap)) as u32
}

/// The flush pitch along `direction`, falling back to the set's extent along
/// that direction when the exact computation has nothing to say.
///
/// The fallback matters for a selection whose pieces never overlap under any
/// translation (a single degenerate sliver, say): stepping by the extent is
/// always a safe, sensible answer.
pub fn contact_pitch_or_extent(pieces: &[Vec<Vec2>], direction: Vec2) -> f32 {
    let d = direction.normalize_or_zero();
    if d == Vec2::ZERO {
        return 0.0;
    }
    match contact_pitch(pieces, d) {
        Some(p) if p > 1e-6 => p,
        _ => extent_along(pieces, d),
    }
}

/// The set's total width along `direction`.
pub fn extent_along(pieces: &[Vec<Vec2>], direction: Vec2) -> f32 {
    let d = direction.normalize_or_zero();
    if d == Vec2::ZERO {
        return 0.0;
    }
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    for piece in pieces {
        for p in piece {
            let v = p.dot(d);
            min = min.min(v);
            max = max.max(v);
        }
    }
    if min > max { 0.0 } else { max - min }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_4;

    /// An axis-aligned rectangle as a convex piece.
    fn rect(center: Vec2, half: Vec2) -> Vec<Vec2> {
        vec![
            center + Vec2::new(-half.x, -half.y),
            center + Vec2::new(half.x, -half.y),
            center + Vec2::new(half.x, half.y),
            center + Vec2::new(-half.x, half.y),
        ]
    }

    #[test]
    fn one_block_steps_by_its_own_width() {
        // The headline case: drag a single 2×1 block sideways and the copies
        // should tile with no seam.
        let piece = vec![rect(Vec2::ZERO, Vec2::new(1.0, 0.5))];
        let pitch = contact_pitch(&piece, Vec2::X).expect("a pitch exists");
        assert!((pitch - 2.0).abs() < 1e-4, "got {pitch}");
        let up = contact_pitch(&piece, Vec2::Y).expect("a pitch exists");
        assert!((up - 1.0).abs() < 1e-4, "got {up}");
    }

    #[test]
    fn a_stack_steps_by_the_whole_stack() {
        // Two unit blocks, one resting on the other: dragging up must step by
        // the full two-block height, not one block.
        let pieces = vec![
            rect(Vec2::new(0.0, 0.5), Vec2::splat(0.5)),
            rect(Vec2::new(0.0, 1.5), Vec2::splat(0.5)),
        ];
        let pitch = contact_pitch(&pieces, Vec2::Y).expect("a pitch exists");
        assert!((pitch - 2.0).abs() < 1e-4, "got {pitch}");
    }

    #[test]
    fn a_gap_inside_the_selection_is_preserved_not_closed() {
        // Two blocks a metre apart: the array must step past *both*, so the
        // copy clears the far one. Stepping by only the near block's width
        // would drop a copy on top of the far one.
        let pieces = vec![
            rect(Vec2::new(0.0, 0.0), Vec2::splat(0.5)),
            rect(Vec2::new(2.0, 0.0), Vec2::splat(0.5)),
        ];
        let pitch = contact_pitch(&pieces, Vec2::X).expect("a pitch exists");
        assert!((pitch - 3.0).abs() < 1e-4, "got {pitch}");
    }

    #[test]
    fn interlocking_pieces_step_closer_than_their_bounding_box() {
        // A staircase: two blocks offset diagonally. Stepping right by the
        // full bounding width would leave a gap the copy could nest into, so
        // the exact pitch must come out *below* the extent. This is the case
        // a bounding-box implementation gets wrong.
        let pieces = vec![
            rect(Vec2::new(0.0, 0.0), Vec2::splat(0.5)),
            rect(Vec2::new(1.0, 1.0), Vec2::splat(0.5)),
        ];
        let pitch = contact_pitch(&pieces, Vec2::X).expect("a pitch exists");
        let extent = extent_along(&pieces, Vec2::X);
        assert!((extent - 2.0).abs() < 1e-4, "extent {extent}");
        assert!(
            pitch < extent - 1e-3,
            "interlocking should beat the bounding box: pitch {pitch} vs extent {extent}"
        );
        assert!(
            (pitch - 1.0).abs() < 1e-4,
            "the blocks nest one apart: {pitch}"
        );
    }

    #[test]
    fn the_pitch_actually_separates_the_copy() {
        // The property that matters, checked directly: at the reported pitch
        // the translated set must not overlap the original.
        let cases: Vec<(Vec<Vec<Vec2>>, Vec2)> = vec![
            (vec![rect(Vec2::ZERO, Vec2::new(1.0, 0.5))], Vec2::X),
            (
                vec![
                    rect(Vec2::new(0.0, 0.5), Vec2::splat(0.5)),
                    rect(Vec2::new(0.0, 1.5), Vec2::splat(0.5)),
                ],
                Vec2::Y,
            ),
            (
                vec![
                    rect(Vec2::ZERO, Vec2::splat(0.5)),
                    rect(Vec2::new(1.0, 1.0), Vec2::splat(0.5)),
                ],
                Vec2::X,
            ),
            (
                vec![rect(Vec2::ZERO, Vec2::new(0.8, 0.3))],
                Vec2::from_angle(FRAC_PI_4),
            ),
        ];
        for (pieces, dir) in cases {
            let d = dir.normalize();
            let pitch = contact_pitch(&pieces, d).expect("a pitch exists");
            // Nudge past the boundary: at exactly the pitch the pair is
            // touching, and float noise could read either way.
            let step = d * (pitch + 1e-3);
            for a in &pieces {
                let moved: Vec<Vec2> = a.iter().map(|p| *p + step).collect();
                for b in &pieces {
                    assert!(
                        crate::sat::penetration(&moved, b, 0.0).is_none(),
                        "the copy still overlaps at pitch {pitch} along {d:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn stepping_short_of_the_pitch_does_overlap() {
        // The pitch is *minimal*, not merely sufficient — otherwise "flush"
        // would leave a gap.
        let pieces = vec![rect(Vec2::ZERO, Vec2::new(1.0, 0.5))];
        let pitch = contact_pitch(&pieces, Vec2::X).expect("a pitch exists");
        let short: Vec<Vec2> = pieces[0]
            .iter()
            .map(|p| *p + Vec2::X * (pitch - 0.05))
            .collect();
        assert!(crate::sat::penetration(&short, &pieces[0], 0.0).is_some());
    }

    #[test]
    fn a_diagonal_direction_measures_the_diagonal_width() {
        // A unit square stepped along 45°: the flush pitch is the diagonal.
        let pieces = vec![rect(Vec2::ZERO, Vec2::splat(0.5))];
        let d = Vec2::from_angle(FRAC_PI_4);
        let pitch = contact_pitch(&pieces, d).expect("a pitch exists");
        assert!(
            (pitch - 2.0_f32.sqrt()).abs() < 1e-4,
            "expected the diagonal, got {pitch}"
        );
    }

    #[test]
    fn an_empty_or_degenerate_request_is_none_rather_than_a_panic() {
        assert!(contact_pitch(&[], Vec2::X).is_none());
        assert!(contact_pitch(&[rect(Vec2::ZERO, Vec2::ONE)], Vec2::ZERO).is_none());
    }

    #[test]
    fn a_tapered_pitch_is_smaller_than_the_flush_one() {
        // Shrinking copies must sit closer together, or "retain contact"
        // would leave a growing gap exactly where the taper bites.
        let pieces = vec![rect(Vec2::ZERO, Vec2::splat(0.5))];
        let flush = contact_pitch(&pieces, Vec2::X).expect("a pitch");
        let taper = tapered_contact_pitch(&pieces, Vec2::X, Vec2::splat(0.5), Vec2::ZERO, 0.0)
            .expect("a pitch");
        // A unit square against a half-size copy: half-widths 0.5 + 0.25.
        assert!((taper - 0.75).abs() < 1e-4, "got {taper}");
        assert!(taper < flush);
    }

    #[test]
    fn a_geometric_chain_of_tapered_pitches_stays_flush_all_the_way_down() {
        // The property the closed form exists for: place copy k at
        // Q·span(r, k) with each copy scaled by r^k, and *every* consecutive
        // pair must be touching-but-not-overlapping, not just the first.
        let ratio = 0.8_f32;
        let base = vec![rect(Vec2::ZERO, Vec2::new(0.6, 0.4))];
        let scale = Vec2::splat(ratio);
        let q = tapered_contact_pitch(&base, Vec2::X, scale, Vec2::ZERO, 0.0).expect("a pitch");

        let copy = |k: u32| -> Vec<Vec<Vec2>> {
            let s = Vec2::splat(ratio.powf(k as f32));
            let shifted = scale_pieces(&base, s, Vec2::ZERO, 0.0);
            let dx = Vec2::X * q * geometric_span(ratio, k);
            shifted
                .iter()
                .map(|p| p.iter().map(|v| *v + dx).collect())
                .collect()
        };
        for k in 0..5 {
            let (a, b) = (copy(k), copy(k + 1));
            for pa in &a {
                for pb in &b {
                    // Flush is two-sided: not interpenetrating, but *within* a
                    // hair's breadth — a gap that grew with k would fail the
                    // second half. The overlap side needs a tolerance because
                    // pieces that touch exactly read as either sign.
                    assert!(
                        crate::sat::penetration(pb, pa, 0.0).is_none_or(|m| m.depth < 1e-4),
                        "copies {k}/{} overlap",
                        k + 1
                    );
                    assert!(
                        crate::sat::penetration(pb, pa, 1e-3).is_some(),
                        "copies {k}/{} drifted apart",
                        k + 1
                    );
                }
            }
        }
    }

    #[test]
    fn per_axis_taper_only_shrinks_the_pitch_of_the_axis_it_names() {
        // The grid case: scaling x alone must leave the vertical pitch alone.
        let pieces = vec![rect(Vec2::ZERO, Vec2::splat(0.5))];
        let narrow = Vec2::new(0.5, 1.0);
        let x = tapered_contact_pitch(&pieces, Vec2::X, narrow, Vec2::ZERO, 0.0).expect("a pitch");
        let y = tapered_contact_pitch(&pieces, Vec2::Y, narrow, Vec2::ZERO, 0.0).expect("a pitch");
        assert!((x - 0.75).abs() < 1e-4, "x pitch shrinks: {x}");
        assert!((y - 1.0).abs() < 1e-4, "y pitch is untouched: {y}");
    }

    #[test]
    fn a_converging_taper_has_a_finite_reach() {
        // Copies at 0.5^k cover at most 2 pitches however far you drag, so
        // the count has to saturate at the cap instead of running away.
        assert_eq!(copies_within(1e9, 1.0, 0.5, 512), 512);
        assert_eq!(copies_within(1.4, 1.0, 0.5, 512), 1, "1 + 0.5 = 1.5 > 1.4");
        assert_eq!(copies_within(1.6, 1.0, 0.5, 512), 2);
        // An inert ratio must agree with plain division.
        assert_eq!(copies_within(3.4, 1.0, 1.0, 512), 3);
        assert_eq!(copies_within(-1.0, 1.0, 1.0, 512), 0);
        assert_eq!(copies_within(10.0, 0.0, 1.0, 512), 0);
    }

    #[test]
    fn the_geometric_span_degrades_to_counting() {
        assert!((geometric_span(1.0, 4) - 4.0).abs() < 1e-6);
        assert!((geometric_span(0.5, 3) - 1.75).abs() < 1e-6); // 1 + .5 + .25
        assert!((geometric_span(2.0, 3) - 7.0).abs() < 1e-5); // 1 + 2 + 4
        assert!((geometric_span(0.9, 0)).abs() < 1e-6);
    }

    #[test]
    fn the_extent_fallback_covers_a_set_that_never_self_overlaps() {
        // A single zero-height sliver never overlaps itself under any
        // horizontal shift, so the exact answer is "no constraint" — the
        // fallback has to supply something usable.
        let sliver = vec![vec![Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)]];
        let pitch = contact_pitch_or_extent(&sliver, Vec2::Y);
        assert!(pitch >= 0.0, "never negative");
        let along = contact_pitch_or_extent(&sliver, Vec2::X);
        assert!(
            (along - 2.0).abs() < 1e-4,
            "falls back to the extent: {along}"
        );
    }
}
