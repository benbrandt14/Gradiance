//! A tiny deterministic PRNG (splitmix64).
//!
//! The stochastic solvers must be **reproducible**: the same seed and the
//! same selection have to produce the same arrangement, or an undo/redo and
//! a re-run would disagree and the user could never dial a result in. That
//! rules out any ambient/global generator, and it is not worth a dependency
//! — splitmix64 is nine lines and passes the statistical bar for jitter and
//! Metropolis draws by a wide margin.

/// A seedable splitmix64 generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator from `seed` (every seed is valid, including 0).
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// The raw next word.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform float in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        // Top 24 bits — exactly the f32 mantissa, so the result is uniform
        // over representable values rather than biased by rounding.
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    /// A uniform float in `[-1, 1)`.
    pub fn signed(&mut self) -> f32 {
        self.unit().mul_add(2.0, -1.0)
    }

    /// A uniform float in `[lo, hi)` (returns `lo` when the range is empty).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        if hi <= lo {
            return lo;
        }
        lo + self.unit() * (hi - lo)
    }

    /// A uniform index in `0..n` (returns 0 when `n` is 0).
    pub fn index(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// An approximately standard-normal draw (sum of 4 uniforms — cheap,
    /// bounded, and plenty for proposal noise).
    pub fn gaussian(&mut self) -> f32 {
        let sum: f32 = (0..4).map(|_| self.signed()).sum();
        // Variance of a sum of 4 uniform(-1,1) is 4/3; scale to ~1.
        sum * 0.866
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_replays_the_same_stream() {
        let a: Vec<f32> = (0..32).scan(Rng::new(7), |r, _| Some(r.unit())).collect();
        let b: Vec<f32> = (0..32).scan(Rng::new(7), |r, _| Some(r.unit())).collect();
        assert_eq!(a, b);
        let c: Vec<f32> = (0..32).scan(Rng::new(8), |r, _| Some(r.unit())).collect();
        assert_ne!(a, c, "different seeds must diverge");
    }

    #[test]
    fn unit_draws_stay_in_range_and_spread_out() {
        let mut rng = Rng::new(42);
        let mut buckets = [0u32; 10];
        for _ in 0..10_000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u));
            buckets[(u * 10.0) as usize] += 1;
        }
        // Every decile should be within a factor of two of 1000.
        for (i, count) in buckets.iter().enumerate() {
            assert!(*count > 500 && *count < 2000, "decile {i} had {count}");
        }
    }

    #[test]
    fn index_stays_in_bounds_including_the_empty_case() {
        let mut rng = Rng::new(3);
        for _ in 0..1000 {
            assert!(rng.index(5) < 5);
        }
        assert_eq!(rng.index(0), 0);
    }

    #[test]
    fn range_handles_an_inverted_span() {
        let mut rng = Rng::new(1);
        assert!((rng.range(2.0, 1.0) - 2.0).abs() < f32::EPSILON);
    }
}
