//! planning-engine: pure, deterministic financial math.
//!
//! Design rules:
//! - No external dependencies (ports to WASM / UniFFI unchanged).
//! - Ledger amounts are integer minor units (`i64` cents); simulation may use
//!   `f64` where continuous compounding math demands it, and results that leave
//!   this crate are rounded to whole cents.
//! - All randomness flows through seeded, platform-independent PRNGs so a
//!   `(seed, inputs)` pair always reproduces the same run.

pub mod amortize;
pub mod monte_carlo;

/// Deterministic xorshift64* PRNG. Small, fast, reproducible across platforms.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Avoid the all-zero fixed point.
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        // 53 bits of mantissa for full double precision.
        ((self.next_u64() >> 11) as f64) / (1u64 << 53) as f64
    }

    /// Standard normal via Box-Muller.
    pub fn next_normal(&mut self) -> f64 {
        let u1 = self.next_f64().max(f64::MIN_POSITIVE);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_reproducible() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn normal_has_sane_moments() {
        let mut rng = Rng::new(7);
        let n = 200_000;
        let mut sum = 0.0;
        let mut sum_sq = 0.0;
        for _ in 0..n {
            let x = rng.next_normal();
            sum += x;
            sum_sq += x * x;
        }
        let mean = sum / n as f64;
        let var = sum_sq / n as f64 - mean * mean;
        assert!(mean.abs() < 0.02, "mean {mean}");
        assert!((var - 1.0).abs() < 0.05, "var {var}");
    }
}
