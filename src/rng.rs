//! Tiny deterministic PRNG (SplitMix64).
//!
//! Determinism is not a convenience here — it is core law #5: every state must
//! be replayable. A scenario is fully reproducible from its seed, so any
//! incident the experiment produces can be re-run bit-for-bit.

pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        // SplitMix64
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform f64 in [lo, hi).
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// Bernoulli draw: true with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn deterministic_for_a_seed() {
        let mut a = Rng::new(12345);
        let mut b = Rng::new(12345);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let diffs = (0..100).filter(|_| a.next_u64() != b.next_u64()).count();
        assert!(diffs > 90, "streams should almost always differ");
    }

    #[test]
    fn f64_lies_in_unit_interval() {
        let mut r = Rng::new(42);
        for _ in 0..10_000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn range_respects_bounds() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let x = r.range_f64(1.5, 5.0);
            assert!((1.5..5.0).contains(&x));
        }
    }

    #[test]
    fn chance_extremes_are_total() {
        let mut r = Rng::new(9);
        for _ in 0..1000 {
            assert!(r.chance(1.0));
            assert!(!r.chance(0.0));
        }
    }
}
