//! Demo data seeder. Runs at startup unless `SEED_DEMO=0`.
//!
//! Credentials: demo@ultimatefinance.app / demo1234
//! Only seeds when the demo user doesn't exist, so restarts are safe.

use crate::models::{Account, AccountType, ReviewState, Transaction, TxSource, Visibility};
use crate::store::MemoryStore;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

pub const DEMO_EMAIL: &str = "demo@ultimatefinance.app";
pub const DEMO_PASSWORD: &str = "demo1234";

pub fn run(store: &MemoryStore) {
    if store.user_by_email(DEMO_EMAIL).is_some() {
        return;
    }

    let hash = crate::auth::hash_password(DEMO_PASSWORD).expect("demo hash");
    let user = store
        .create_user(DEMO_EMAIL, hash, "Demo Blake".into())
        .expect("seed user");
    let hh = store.create_household("Demo Household".into(), "USD", user.id);

    let b = Seeder { store, hh: hh.id, owner: user.id };

    // ---- accounts ---------------------------------------------------------
    let checking = b.account("Everyday Checking", AccountType::Checking, 240_000);
    let savings = b.account("High-Yield Savings", AccountType::Savings, 1_850_000);
    let card = b.account("Sapphire Credit Card", AccountType::CreditCard, -84_216);
    let brokerage = b.account("Brokerage", AccountType::Brokerage, 2_412_055);
    let car_loan = b.account("Car Loan", AccountType::Loan, -1_148_000);

    // ---- 3 months of history ---------------------------------------------
    for m in (1..=3).rev() {
        let month_start = 30 * (m - 1);

        // Income: semi-monthly salary.
        b.tx(checking, month_start + 1, 425_000, Some("Acme Corp — Payroll"), true);
        b.tx(checking, month_start + 15, 425_000, Some("Acme Corp — Payroll"), true);
        // Freelance windfall two months back.
        if m == 3 {
            b.tx(checking, month_start + 9, 120_000, Some("Consulting Invoice #204"), true);
        }

        // Fixed costs.
        b.tx(checking, month_start + 1, -165_000, Some("Skyline Apartments — Rent"), true);
        b.tx(checking, month_start + 18, -38_500, Some("Auto Loan Payment"), true);
        b.tx(car_loan, month_start + 18, 38_500, Some("Payment posted"), true);
        b.tx(checking, month_start + 22, -11_200, Some("Volt Energy — Electric"), true);
        b.tx(checking, month_start + 22, -6_500, Some("Fiberline Internet"), true);

        // Subscriptions (card).
        b.tx(card, month_start + 3, -1_299, Some("Streamly"), true);
        b.tx(card, month_start + 7, -2_999, Some("GymLife Membership"), true);
        b.tx(card, month_start + 12, -299, Some("CloudVault Storage"), true);
        b.tx(card, month_start + 14, -1_099, Some("Streamly Music"), true);

        // Card payment from checking.
        b.tx(checking, month_start + 20, -60_000, Some("Sapphire Card Payment"), true);
        b.tx(card, month_start + 20, 60_000, Some("Payment received"), true);

        // Savings automation + interest.
        b.tx(checking, month_start + 2, -50_000, Some("Transfer to Savings"), true);
        b.tx(savings, month_start + 2, 50_000, Some("Auto-save"), true);
        b.tx(savings, month_start + 28, 31_20, Some("Interest earned"), true);

        // Investing.
        b.tx(brokerage, month_start + 5, -100_000, Some("VTI — Buy 2 shares"), true);
        if m % 2 == 0 {
            b.tx(brokerage, month_start + 25, 4_530, Some("Dividend — VTI"), true);
        }

        // Variable spend, deterministic pseudo-random amounts.
        for week in 0..4u32 {
            let w = month_start + 7 * week as i64;
            b.tx(card, w + 2, -8_500 - (week as i64 * 1_250) % 4_000, Some("Grocery Mart"), m > 1);
            b.tx(card, w + 3, -4_50 - (week as i64 * 90) % 300, Some("Coffee Bar"), m > 1);
            b.tx(card, w + 5, -3_200 - (week as i64 * 2_100) % 5_000, Some("Trattoria Roma"), m > 1);
            b.tx(checking, w + 4, -4_100 - (week as i64 * 700) % 1_500, Some("Gas N Go"), m > 1);
        }
    }

    // A couple of fresh, unreviewed items so the review workflow has work.
    b.tx(card, 1, -6_789, Some("MegaMart Electronics"), false);
    b.tx(card, 0, -49_99, Some("Streamly"), false);
    b.tx(checking, 0, -23_75, Some("Coffee Bar"), false);

    tracing::info!(email = DEMO_EMAIL, "demo data seeded");
}

struct Seeder<'a> {
    store: &'a MemoryStore,
    hh: Uuid,
    owner: Uuid,
}

impl Seeder<'_> {
    fn account(&self, name: &str, ty: AccountType, balance: i64) -> Uuid {
        let acct = Account {
            id: Uuid::new_v4(),
            household_id: self.hh,
            connection_id: None,
            external_id: None,
            created_by: self.owner,
            account_type: ty,
            name: name.into(),
            currency: "USD".into(),
            current_balance_minor: balance,
            visibility: Visibility::AllMembers,
            created_at: OffsetDateTime::now_utc(),
        };
        self.store.insert_account(acct).expect("seed account").id
    }

    #[allow(clippy::too_many_arguments)]
    fn tx(&self, account: Uuid, days_ago: i64, amount: i64, merchant: Option<&str>, reviewed: bool) {
        let today = OffsetDateTime::now_utc().date();
        let posted: Date = today - Duration::days(days_ago);
        let txn = Transaction {
            id: Uuid::new_v4(),
            household_id: self.hh,
            account_id: account,
            external_id: None,
            posted_on: posted,
            amount_minor: amount,
            merchant_raw: merchant.map(str::to_string),
            description: None,
            notes: None,
            review_state: if reviewed { ReviewState::Reviewed } else { ReviewState::Unreviewed },
            source: TxSource::Manual,
            created_by: self.owner,
            created_at: OffsetDateTime::now_utc(),
        };
        self.store.create_transaction(txn).expect("seed txn");
    }
}
