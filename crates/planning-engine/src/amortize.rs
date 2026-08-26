//! Loan amortization with exact cent-level accounting.

/// Loan terms. `annual_rate_pct` is e.g. 6.0 for 6% APR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoanTerms {
    pub principal_cents: i64,
    pub annual_rate_pct: f64,
    pub months: u32,
}

/// One month of an amortization schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmortRow {
    /// 1-based payment number.
    pub month: u32,
    /// Total paid this month (interest + principal), cents.
    pub payment_cents: i64,
    pub interest_cents: i64,
    pub principal_cents: i64,
    /// Remaining balance *after* this payment, cents.
    pub balance_cents: i64,
}

/// Round half away from zero (banker's rounding surprises users; this matches
/// how statements present amounts).
fn round_cents(x: f64) -> i64 {
    if x >= 0.0 {
        (x + 0.5).floor() as i64
    } else {
        (x - 0.5).ceil() as i64
    }
}

/// Standard fully-amortizing monthly payment.
pub fn monthly_payment(terms: LoanTerms) -> i64 {
    let r = terms.annual_rate_pct / 100.0 / 12.0;
    let n = f64::from(terms.months);
    let p = terms.principal_cents as f64;
    let raw = if r.abs() < f64::EPSILON {
        p / n
    } else {
        p * r / (1.0 - (1.0 + r).powf(-n))
    };
    round_cents(raw)
}

/// Full schedule with optional extra principal each month.
///
/// The final payment is adjusted so the loan closes at exactly zero — the sum
/// of all principal payments equals the original principal to the cent.
pub fn amortization_schedule(terms: LoanTerms, extra_monthly_cents: i64) -> Vec<AmortRow> {
    assert!(terms.principal_cents > 0 && terms.months > 0);
    let r = terms.annual_rate_pct / 100.0 / 12.0;
    let mut balance = terms.principal_cents;
    let base_payment = monthly_payment(terms);
    let mut rows = Vec::with_capacity(terms.months as usize);

    for month in 1..=terms.months {
        let interest = round_cents(balance as f64 * r);
        // Never pay more than needed in the final stretch.
        let mut principal = (base_payment - interest + extra_monthly_cents).min(balance);
        // Cent-rounded payments can leave a small residual at term end; the
        // final payment absorbs it so the loan closes at exactly zero.
        if month == terms.months {
            principal = balance;
        }
        if principal <= 0 {
            // Payment doesn't cover interest — negative amortization guard.
            panic!("payment must cover interest");
        }
        balance -= principal;
        rows.push(AmortRow {
            month,
            payment_cents: interest + principal,
            interest_cents: interest,
            principal_cents: principal,
            balance_cents: balance,
        });
        if balance == 0 {
            break;
        }
    }
    debug_assert_eq!(
        rows.iter().map(|r| r.principal_cents).sum::<i64>(),
        terms.principal_cents,
        "principal must amortize exactly"
    );
    rows
}

/// Total interest over a schedule.
pub fn total_interest(rows: &[AmortRow]) -> i64 {
    rows.iter().map(|r| r.interest_cents).sum()
}

/// Side-by-side refi/extra-payment comparison.
#[derive(Debug, Clone, Copy)]
pub struct Comparison {
    pub months_to_payoff: u32,
    pub total_interest_cents: i64,
}

pub fn compare(terms: LoanTerms, extra_monthly_cents: i64) -> Comparison {
    let rows = amortization_schedule(terms, extra_monthly_cents);
    Comparison {
        months_to_payoff: rows.len() as u32,
        total_interest_cents: total_interest(&rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classic_30y() -> LoanTerms {
        LoanTerms { principal_cents: 30_000_000, annual_rate_pct: 6.0, months: 360 }
    }

    #[test]
    fn known_payment_value() {
        // $300k @ 6% / 30y → $1,798.65/mo (standard published value).
        assert_eq!(monthly_payment(classic_30y()), 179_865);
    }

    #[test]
    fn schedule_amortizes_exactly() {
        let rows = amortization_schedule(classic_30y(), 0);
        assert_eq!(rows.len(), 360);
        assert_eq!(rows.last().unwrap().balance_cents, 0);
        let principal_paid: i64 = rows.iter().map(|r| r.principal_cents).sum();
        assert_eq!(principal_paid, 30_000_000);
        // Interest ≈ $347,515 over 30 years (±$5 tolerance for cent rounding).
        let interest = total_interest(&rows);
        assert!((interest - 34_751_500).abs() <= 500, "interest {interest}");
    }

    #[test]
    fn extra_payments_save_interest_and_time() {
        let base = compare(classic_30y(), 0);
        let extra = compare(classic_30y(), 20_000); // +$200/mo
        assert!(extra.total_interest_cents < base.total_interest_cents);
        assert!(extra.months_to_payoff < base.months_to_payoff);
        // Roughly $60k saved and ~6 years earlier — sanity band, not exact.
        assert!(base.total_interest_cents - extra.total_interest_cents > 50_000_00);
    }

    #[test]
    fn zero_rate_loan() {
        let terms = LoanTerms { principal_cents: 12_000, annual_rate_pct: 0.0, months: 12 };
        assert_eq!(monthly_payment(terms), 1_000);
        let rows = amortization_schedule(terms, 0);
        assert_eq!(total_interest(&rows), 0);
        assert_eq!(rows.len(), 12);
    }

    #[test]
    fn early_payoff_adjusts_final_payment() {
        let terms = LoanTerms { principal_cents: 10_000_000, annual_rate_pct: 5.0, months: 360 };
        let rows = amortization_schedule(terms, 90_000); // huge extra → early close
        assert!(rows.len() < 360);
        assert_eq!(rows.last().unwrap().balance_cents, 0);
        assert_eq!(rows.iter().map(|r| r.principal_cents).sum::<i64>(), terms.principal_cents);
    }
}
