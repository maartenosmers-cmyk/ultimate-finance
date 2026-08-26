//! Monte Carlo retirement / FIRE modeling in real (inflation-adjusted) dollars.

use crate::Rng;

/// Simulation inputs. All dollar values are real (today's) dollars, cents.
#[derive(Debug, Clone, Copy)]
pub struct RetirementInputs {
    /// Current investable portfolio.
    pub starting_portfolio_cents: i64,
    /// Annual additions until `years_to_retirement` elapses.
    pub annual_contribution_cents: i64,
    /// Annual spending during retirement (real).
    pub annual_spending_cents: i64,
    pub years_to_retirement: u32,
    /// Years of retirement to model after retiring.
    pub retirement_years: u32,
    /// Arithmetic mean *real* annual return, e.g. 0.05 for 5% real.
    pub mean_real_return: f64,
    /// Annual standard deviation of real returns, e.g. 0.16.
    pub return_stddev: f64,
}

/// Aggregate results across all simulated paths.
#[derive(Debug, Clone, Copy)]
pub struct RetirementOutcome {
    pub paths_simulated: u32,
    /// Fraction of paths that never run out of money through the full horizon.
    pub success_probability: f64,
    /// Terminal portfolio value percentiles (cents).
    pub terminal_p05_cents: i64,
    pub terminal_p50_cents: i64,
    pub terminal_p95_cents: i64,
    /// Median portfolio at the moment of retirement (cents).
    pub nest_egg_p50_cents: i64,
}

fn round_cents(x: f64) -> i64 {
    if x >= 0.0 { (x + 0.5).floor() as i64 } else { (x - 0.5).ceil() as i64 }
}

/// One simulated lifetime → (depleted?, nest egg at retirement, terminal value).
///
/// Sequence per year: market return applies first, then the year's cash flow.
/// Failed paths report a terminal value of zero (they ran out), which feeds
/// honestly into the percentile fan.
fn run_path(rng: &mut Rng, i: &RetirementInputs) -> (bool, i64, i64) {
    let mut balance = i.starting_portfolio_cents as f64;
    let mut nest_egg = balance;
    let total_years = i.years_to_retirement + i.retirement_years;

    for year in 0..total_years {
        let shock = i.mean_real_return + i.return_stddev * rng.next_normal();
        // Clamp tails so one draw can't wipe out >90% or exceed +50% real.
        balance *= 1.0 + shock.clamp(-0.9, 0.5);

        if year < i.years_to_retirement {
            balance += i.annual_contribution_cents as f64;
        } else {
            balance -= i.annual_spending_cents as f64;
            if balance < 0.0 {
                return (true, 0, 0);
            }
        }

        if year + 1 == i.years_to_retirement {
            nest_egg = balance;
        }
    }

    (false, round_cents(nest_egg), round_cents(balance))
}

/// Run `paths` independent lifetime simulations with a deterministic seed:
/// `(seed, inputs)` always reproduces the same outcome.
pub fn simulate_retirement(inputs: RetirementInputs, paths: u32, seed: u64) -> RetirementOutcome {
    assert!(paths > 0);
    let mut rng = Rng::new(seed);
    let mut terminals = vec![0i64; paths as usize];
    let mut nest_eggs = vec![0i64; paths as usize];
    let mut successes = 0u32;

    for idx in 0..paths as usize {
        let (depleted, nest_egg, terminal) = run_path(&mut rng, &inputs);
        terminals[idx] = terminal;
        nest_eggs[idx] = nest_egg;
        if !depleted {
            successes += 1;
        }
    }

    terminals.sort_unstable();
    nest_eggs.sort_unstable();
    let pct = |v: &[i64], q: f64| v[((v.len() as f64 - 1.0) * q).round() as usize];

    RetirementOutcome {
        paths_simulated: paths,
        success_probability: successes as f64 / paths as f64,
        terminal_p05_cents: pct(&terminals, 0.05),
        terminal_p50_cents: pct(&terminals, 0.50),
        terminal_p95_cents: pct(&terminals, 0.95),
        nest_egg_p50_cents: pct(&nest_eggs, 0.50),
    }
}

/// Solve "what annual savings do I need to hit `target_success`?" via bisection.
/// Deterministic given seed. Returns 0 if already achievable without saving;
/// returns the cap if unreachable within it (caller decides what to show).
pub fn required_annual_savings(mut inputs: RetirementInputs, target_success: f64, seed: u64) -> i64 {
    const PATHS: u32 = 4_000;
    // Cap heuristic: you can't save more than you spend each year.
    let cap = inputs.annual_spending_cents.max(10_000_00);
    let mut prob_at = |contribution: i64| {
        inputs.annual_contribution_cents = contribution;
        simulate_retirement(inputs, PATHS, seed).success_probability
    };

    if prob_at(0) >= target_success {
        return 0;
    }
    if prob_at(cap) < target_success {
        return cap;
    }

    let (mut lo, mut hi) = (0i64, cap);
    while hi - lo > 100 { // converge to within $1
        let mid = (lo + hi) / 2;
        if prob_at(mid) >= target_success { hi = mid; } else { lo = mid; }
    }
    hi
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_inputs() -> RetirementInputs {
        RetirementInputs {
            starting_portfolio_cents: 50_000_000,   // $500k
            annual_contribution_cents: 20_000_00,   // $20k/yr
            annual_spending_cents: 60_000_00,       // $60k/yr
            years_to_retirement: 25,
            retirement_years: 40,
            mean_real_return: 0.05,
            return_stddev: 0.16,
        }
    }

    #[test]
    fn higher_contributions_raise_success() {
        let mut a = base_inputs();
        a.annual_contribution_cents = 10_000_00;
        let mut b = a;
        b.annual_contribution_cents = 30_000_00;
        let pa = simulate_retirement(a, 5_000, 12345).success_probability;
        let pb = simulate_retirement(b, 5_000, 12345).success_probability;
        assert!(pb > pa, "pb {pb} should exceed pa {pa}");
    }

    #[test]
    fn impossible_plan_has_zero_success() {
        let mut i = base_inputs();
        i.annual_spending_cents = 1_000_000_000_0; // $10M/yr spending
        let out = simulate_retirement(i, 2_000, 99);
        assert!(out.success_probability < 0.02, "got {}", out.success_probability);
    }

    #[test]
    fn guaranteed_plan_has_full_success() {
        let mut i = base_inputs();
        i.mean_real_return = 0.20;
        i.return_stddev = 0.0;
        i.annual_spending_cents = 10_000_00;
        let out = simulate_retirement(i, 500, 1);
        assert_eq!(out.success_probability, 1.0);
        assert!(out.terminal_p05_cents > 0);
    }

    #[test]
    fn immediate_retirement_snapshots_nest_egg_correctly() {
        let i = RetirementInputs {
            starting_portfolio_cents: 100_000_000, // $1M
            annual_contribution_cents: 0,
            annual_spending_cents: 1_000_000,      // $10k/yr
            years_to_retirement: 0,
            retirement_years: 30,
            mean_real_return: 0.0,
            return_stddev: 0.0,
        };
        let out = simulate_retirement(i, 100, 5);
        // Deterministic 0% real: survives all 30y, ending with 1M − 30×10k.
        assert_eq!(out.success_probability, 1.0);
        assert_eq!(out.nest_egg_p50_cents, 100_000_000);
        assert_eq!(out.terminal_p50_cents, 70_000_000);
    }

    #[test]
    fn solver_result_meets_target() {
        let mut i = base_inputs();
        i.annual_contribution_cents = 0;
        i.annual_spending_cents = 40_000_00;
        let need = required_annual_savings(i, 0.85, 777);
        assert!(need > 0 && need <= i.annual_spending_cents);
        let mut check = i;
        check.annual_contribution_cents = need;
        let p = simulate_retirement(check, 8_000, 777).success_probability;
        assert!(p >= 0.85, "solver result only achieved p={p}");
    }

    #[test]
    fn percentiles_ordered_and_deterministic() {
        let i = base_inputs();
        let a = simulate_retirement(i, 3_000, 2024);
        let b = simulate_retirement(i, 3_000, 2024);
        assert_eq!(a.success_probability, b.success_probability);
        assert_eq!(a.terminal_p50_cents, b.terminal_p50_cents);
        assert!(a.terminal_p05_cents <= a.terminal_p50_cents);
        assert!(a.terminal_p50_cents <= a.terminal_p95_cents);
        assert!((0.0..=1.0).contains(&a.success_probability));
    }
}
