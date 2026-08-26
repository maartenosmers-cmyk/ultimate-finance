//! Bank aggregation. One orchestration layer, pluggable providers.
//!
//! Balance semantics: synced accounts are *provider-authoritative* — after a
//! sync, `current_balance_minor` equals what the institution reports, and
//! transaction inserts on synced accounts never adjust balances locally.

pub mod mock;
pub mod plaid;

use crate::models::{Connection, ConnectionStatus};
use time::Date;
use uuid::Uuid;

/// Provider-normalized account snapshot from a sync.
#[derive(Debug, Clone)]
pub struct ExtAccount {
    pub external_id: String,
    pub name: String,
    pub account_type: crate::models::AccountType,
    pub currency: String,
    pub current_balance_minor: i64,
}

/// Provider-normalized transaction from a sync.
#[derive(Debug, Clone)]
pub struct ExtTxn {
    pub external_id: String,
    /// Provider-side account id within this connection.
    pub account_external_id: String,
    pub posted_on: Date,
    /// Signed cents: income positive, expense negative (provider conventions
    /// are normalized by each adapter).
    pub amount_minor: i64,
    pub merchant_raw: Option<String>,
    pub description: Option<String>,
}

pub struct SyncPage {
    pub txns: Vec<ExtTxn>,
    pub next_cursor: String,
    pub has_more: bool,
}

#[derive(Clone)]
pub enum Provider {
    Plaid(plaid::PlaidClient),
    Mock(mock::MockAggregator),
}

impl Provider {
    pub fn name(&self) -> &'static str {
        match self {
            Provider::Plaid(_) => "plaid",
            Provider::Mock(_) => "mock",
        }
    }

    async fn accounts(&self, conn: &Connection) -> Result<Vec<ExtAccount>, AggregatorError> {
        match self {
            Provider::Plaid(c) => c.accounts(conn).await,
            Provider::Mock(m) => m.accounts(conn),
        }
    }

    async fn transactions(
        &self,
        conn: &Connection,
        cursor: Option<&str>,
    ) -> Result<SyncPage, AggregatorError> {
        match self {
            Provider::Plaid(c) => c.transactions(conn, cursor).await,
            Provider::Mock(m) => m.transactions(conn, cursor),
        }
    }

    pub(crate) async fn create_link_token(&self, household_id: Uuid) -> Result<String, AggregatorError> {
        match self {
            Provider::Plaid(c) => c.create_link_token(household_id).await,
            Provider::Mock(_) => Err(AggregatorError::Unsupported("mock has no link flow")),
        }
    }

    pub(crate) async fn exchange_public_token(&self, public_token: &str) -> Result<(String, String), AggregatorError> {
        // → (access_token, external_item_id)
        match self {
            Provider::Plaid(c) => c.exchange_public_token(public_token).await,
            Provider::Mock(_) => Err(AggregatorError::Unsupported("mock has no link flow")),
        }
    }
}

#[derive(Clone)]
pub struct Registry {
    pub plaid: Option<plaid::PlaidClient>,
}

#[derive(Debug, thiserror::Error)]
pub enum AggregatorError {
    #[error("provider not configured")]
    NotConfigured,
    #[error("operation unsupported for this provider")]
    Unsupported(&'static str),
    #[error("provider rejected the request")]
    Provider(String),
    #[error("network error")]
    Network(#[from] reqwest::Error),
    #[error("store error")]
    Store(#[from] crate::store::StoreError),
}

impl Registry {
    pub fn get(&self, name: &str) -> Result<Provider, AggregatorError> {
        match name {
            "mock" => Ok(Provider::Mock(mock::MockAggregator)),
            "plaid" => {
                self.plaid
                    .clone()
                    .map(Provider::Plaid)
                    .ok_or(AggregatorError::NotConfigured)
            }
            _ => Err(AggregatorError::Unsupported("unknown provider")),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_ok()
    }
}

/// Pull every pending page and persist accounts + new transactions.
///
/// Returns the number of newly-inserted transactions.
pub async fn sync_connection(
    state: &crate::state::SharedState,
    conn: &Connection,
) -> Result<usize, AggregatorError> {
    let provider = state.providers.get(&conn.provider)?;
    let access = conn
        .access_token
        .clone()
        .ok_or(AggregatorError::Provider("connection missing credential".into()))?;
    let working = Connection { access_token: Some(access), ..conn.clone() };
    // System attribution until per-actor service identity lands.
    let owner = state.store.household_owner(conn.household_id).unwrap_or(conn.household_id);

    // 1) Account snapshots are authoritative for balances.
    for a in provider.accounts(&working).await? {
        state.store.upsert_synced_account(conn.household_id, owner, conn.id, a);
    }

    // 2) Transaction pages until drained (bounded to keep requests snappy).
    let mut inserted = 0usize;
    let mut cursor = conn.cursor.clone();
    let mut pages = 0;
    loop {
        let page = provider.transactions(&working, cursor.as_deref()).await?;
        let account_ids: Vec<(String, Uuid)> = state
            .store
            .accounts_for_connection(conn.id)
            .into_iter()
            .filter_map(|a| a.external_id.map(|e| (e, a.id)))
            .collect();

        for t in page.txns {
            let Some((_, internal_id)) =
                account_ids.iter().find(|(ext, _)| ext == &t.account_external_id)
            else {
                continue;
            };
            let txn = crate::models::Transaction {
                id: Uuid::new_v4(),
                household_id: conn.household_id,
                account_id: *internal_id,
                external_id: Some(t.external_id),
                posted_on: t.posted_on,
                amount_minor: t.amount_minor,
                merchant_raw: t.merchant_raw,
                description: t.description,
                notes: None,
                review_state: crate::models::ReviewState::Unreviewed,
                source: crate::models::TxSource::Aggregate,
                created_by: owner,
                created_at: time::OffsetDateTime::now_utc(),
            };
            if state.store.upsert_synced_transaction(txn) {
                inserted += 1;
            }
        }

        let done = !page.has_more;
        cursor = Some(page.next_cursor);
        if done || pages >= 10 {
            break;
        }
        pages += 1;
    }

    // 3) Persist cursor + status.
    state
        .store
        .update_connection_status(conn.id, ConnectionStatus::Connected, cursor)?;
    Ok(inserted)
}
