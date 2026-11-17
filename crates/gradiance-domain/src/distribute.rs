//! Spreading one value across a set of bodies, between bounds (brief 4b-1).
//!
//! Pure functions: the group operations compute per-member values here and
//! emit one property edit, so the arithmetic is tested without an app (the
//! [`random_fills`](crate::appearance::random_fills) precedent).

/// How [`distribute`] spreads values between `lo` and `hi`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spread {
    /// Evenly in the order of a sort key: the smallest key gets `lo`, the
    /// largest `hi`, equal steps between. Ties keep member order.
    Linear,
    /// Uniformly at random in `[lo, hi]`, reproducible from the seed.
    Random {
        /// The generator's seed: the same seed gives the same values.
        seed: u64,
    },
}

/// One value per member, between `lo` and `hi` (in either order).
///
/// `keys` holds one sort key per member, used by [`Spread::Linear`]; a NaN
/// key sorts last. A single member gets `lo` under `Linear` (the start of
/// the range, not its middle, so "from 0.2" means 0.2). The result is in
/// member order, the same length as `keys`.
#[must_use]
pub fn distribute(keys: &[f64], lo: f64, hi: f64, spread: Spread) -> Vec<f64> {
    let n = keys.len();
    match spread {
        Spread::Linear => {
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|&a, &b| keys[a].total_cmp(&keys[b]).then(a.cmp(&b)));
            let mut out = vec![lo; n];
            if n > 1 {
                let steps = (n - 1) as f64;
                for (rank, &member) in order.iter().enumerate() {
                    // The last rank is `hi` exactly, not `lo + (hi - lo)`.
                    out[member] = if rank + 1 == n {
                        hi
                    } else {
                        lo + (hi - lo) * rank as f64 / steps
                    };
                }
            }
            out
        }
        Spread::Random { seed } => {
            let mut state = seed;
            (0..n)
                .map(|_| {
                    let unit = unit_interval(splitmix64(&mut state));
                    (lo + (hi - lo) * unit).clamp(lo.min(hi), lo.max(hi))
                })
                .collect()
        }
    }
}

/// `SplitMix64`: a tiny, well-mixed generator; enough for authoring spreads
/// and dependency-free, so a seed means the same thing on every platform.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The top 53 bits of `bits` as a value in `[0, 1)`.
fn unit_interval(bits: u64) -> f64 {
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    // float_cmp: the endpoints are assigned, not computed.
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn linear_hits_both_endpoints_in_key_order() {
        let keys = [3.0, 1.0, 2.0];
        let out = distribute(&keys, 0.5, 5.0, Spread::Linear);
        assert_eq!(out, vec![5.0, 0.5, 2.75]);
    }

    #[test]
    fn linear_accepts_reversed_bounds_and_one_member() {
        let out = distribute(&[0.0, 1.0], 1.0, 0.0, Spread::Linear);
        assert_eq!(out, vec![1.0, 0.0]);
        assert_eq!(distribute(&[7.0], 0.2, 0.9, Spread::Linear), vec![0.2]);
        assert_eq!(distribute(&[], 0.0, 1.0, Spread::Linear), Vec::<f64>::new());
    }

    #[test]
    fn linear_ties_keep_member_order() {
        let out = distribute(&[1.0, 1.0, 1.0], 0.0, 2.0, Spread::Linear);
        assert_eq!(out, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn random_stays_in_bounds_and_is_reproducible() {
        let keys = [0.0; 64];
        let a = distribute(&keys, -2.0, 3.0, Spread::Random { seed: 7 });
        let b = distribute(&keys, -2.0, 3.0, Spread::Random { seed: 7 });
        let c = distribute(&keys, -2.0, 3.0, Spread::Random { seed: 8 });
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.iter().all(|v| (-2.0..=3.0).contains(v)));
        // Reversed bounds are the same range.
        let r = distribute(&keys, 3.0, -2.0, Spread::Random { seed: 7 });
        assert!(r.iter().all(|v| (-2.0..=3.0).contains(v)));
        // Spread out, not clumped: both halves of the range are used.
        assert!(a.iter().any(|v| *v < 0.5) && a.iter().any(|v| *v > 0.5));
    }
}
