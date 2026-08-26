//! Deterministic fake institution: same contract as Plaid, zero network.
//! Powers dev flows and proves the whole sync pipeline in CI.

use time::Date;
use time::Month;
use uuid::Uuid;

use super::{ExtAccount, ExtTxn, SyncPage};
use crate::models::{Connection, ConnectionStatus};

#[derive(Clone)]
pub struct MockAggregator;

const PAGE_SIZE: usize = 3;

impl MockAggregator {
    pub fn accounts(&self, _conn: &Connection) -> Result<Vec<ExtAccount>, super::AggregatorError> {
        Ok(vec![
            ExtAccount {
                external_id: "mock-checking".into(),
                name: "Mock Everyday Checking".into(),
                account_type: crate::models::AccountType::Checking,
                currency: "USD".into(),
                current_balance_minor: 152_340,
            },
            ExtAccount {
                external_id: "mock-savings".into(),
                name: "Mock High-Yield Savings".into(),
                account_type: crate::models::AccountType::Savings,
                currency: "USD".into(),
                current_balance_minor: 2_500_000_00,
            },
        ])
    }

    /// 9 deterministic transactions across the two accounts, paginated by an
    /// integer offset cursor so repeated syncs are idempotent.
    pub fn transactions(
        &self,
        conn: &Connection,
        cursor: Option<&str>,
    ) -> Result<SyncPage, super::AggregatorError> {
        let offset: usize = cursor.and_then(|c| c.parse().ok()).unwrap_or(0);
        let all = canned_transactions(conn.id);
        let end = (offset + PAGE_SIZE).min(all.len());
        let slice = all.get(offset..end).unwrap_or_default();
        Ok(SyncPage {
            txns: slice.to_vec(),
            next_cursor: end.to_string(),
            has_more: end < all.len(),
        })
    }
}

fn date(y: i32, m: u8, d: u8) -> Date {
    Date::from_calendar_date(
        y,
        Month::try_from(m).expect("month"),
        d,
    )
    .expect("date")
}

fn canned() -> Vec<(&'static str, &'static str, &'static str, i64, Option<&'static str>)> {
    // (account, ext_id, date, amount_cents signed, merchant)
    vec![
        ("mock-checking", "mc-1", "2026-08-25", -4_250, Some("Coffee Bar")),
        ("mock-checking", "mc-2", "2026-08-24", -89_500, Some("Grocery Mart")),
        ("mock-savings", "ms-1", "2026-08-24", 100_000_00, Some("Transfer from Checking")),
        ("mock-checking", "mc-3", "2026-08-23", -1_299, Some("Streamly")),
        ("mock-checking", "mc-4", "2026-08-22", -45_000, Some("Gas N Go")),
        ("mock-checking", "mc-5", "2026-08-22", -23_750, Some("Pharmacy Plus")),
        ("mock-checking", "mc-6", "2026-08-21", 250_000_00, None), // payroll, no merchant
        ("mock-savings", "ms-2", "2026-08-20", -500_000_00, Some("Vanguard Buy")),
        ("mock-checking", "mc-7", "2026-08-20", -1_299, Some("Streamly")),
    ]
}

fn canned_transactions(conn_id: Uuid) -> Vec<ExtTxn> {
    canned()
        .into_iter()
        .map(|(acct, ext, day, amt, merch)| ExtTxn {
            external_id: format!("{conn_id}-{ext}"),
            account_external_id: acct.into(),
            posted_on: parse_date(day),
            amount_minor: amt,
            merchant_raw: merch.map(str::to_string),
            description: merch.map(|m| format!("{m} purchase")),
        })
        .collect()
}

fn parse_date(s: &str) -> Date {
    let parts: Vec<i32> = s.split('-').map(|p| p.parse().expect("date part")).collect();
    date(parts[0], parts[1] as u8, parts[2] as u8)
}

/// Status is always healthy; kept for symmetry with provider contract tests.
#[allow(dead_code)]
fn default_status() -> ConnectionStatus {
    ConnectionStatus::Connected
}
