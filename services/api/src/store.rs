//! In-memory store: the single persistence facade. Handlers never touch
//! collections directly, so swapping this for SQLx/Postgres later is a
//! mechanical change confined to this file.
//!
//! Invariants enforced here (and only here):
//!   * unique lowercase emails
//!   * transactions atomically adjust their account's running balance
//!   * deleting an account cascades its transactions

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::models::{
    Account, Connection, ConnectionStatus, Household, MemberStatus, Membership, ReviewState, Role,
    Session, StoredUser, Transaction, TxSource, User, Visibility,
};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("email already registered")]
    EmailTaken,
    #[error("{0} not found")]
    NotFound(&'static str),
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Full sort/cursor key for stable pagination.
type TxKey = (Date, OffsetDateTime, Uuid);

fn tx_key(t: &Transaction) -> TxKey {
    (t.posted_on, t.created_at, t.id)
}

#[derive(Default)]
struct Data {
    users: HashMap<Uuid, StoredUser>,
    emails: HashMap<String, Uuid>,
    sessions: HashMap<String, Session>,
    households: HashMap<Uuid, Household>,
    members: HashMap<(Uuid, Uuid), Membership>,
    accounts: HashMap<Uuid, Account>,
    txns: HashMap<Uuid, Transaction>,
    connections: HashMap<Uuid, Connection>,
    /// Dedup ledger for aggregator webhooks: key = `{provider}:{event_key}`.
    webhook_events: HashMap<String, ()>,
}

/// Fields a client may change on an existing transaction.
#[derive(Debug, Default)]
pub struct TxPatch {
    pub amount_minor: Option<i64>,
    pub posted_on: Option<Date>,
    pub merchant_raw: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub review_state: Option<ReviewState>,
}

#[derive(Default)]
pub struct MemoryStore {
    data: RwLock<Data>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Data> {
        self.data.write().expect("store lock poisoned")
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Data> {
        self.data.read().expect("store lock poisoned")
    }

    // ------------------------------------------------------------- users --

    pub fn create_user(
        &self,
        email: &str,
        password_hash: String,
        display_name: String,
    ) -> StoreResult<User> {
        let email = email.trim().to_ascii_lowercase();
        let user =
            User { id: Uuid::new_v4(), email, display_name, created_at: OffsetDateTime::now_utc() };
        let mut d = self.write();
        if d.emails.contains_key(&user.email) {
            return Err(StoreError::EmailTaken);
        }
        d.emails.insert(user.email.clone(), user.id);
        d.users.insert(user.id, StoredUser { user: user.clone(), password_hash });
        Ok(user)
    }

    pub fn user_by_email(&self, email: &str) -> Option<StoredUser> {
        let email = email.trim().to_ascii_lowercase();
        let d = self.read();
        let id = *d.emails.get(&email)?;
        d.users.get(&id).cloned()
    }

    // ---------------------------------------------------------- sessions --

    pub fn create_session(&self, user_id: Uuid, ttl: Duration) -> Session {
        let session = Session {
            token: crate::auth::new_token(),
            user_id,
            expires_at: OffsetDateTime::now_utc() + ttl,
        };
        self.write().sessions.insert(session.token.clone(), session.clone());
        session
    }

    /// Resolve a live session; lazily drops expired ones.
    pub fn resolve_session(&self, token: &str) -> Option<User> {
        let mut d = self.write();
        let session = d.sessions.get(token).cloned()?;
        if OffsetDateTime::now_utc() >= session.expires_at {
            d.sessions.remove(token);
            return None;
        }
        d.users.get(&session.user_id).map(|u| u.user.clone())
    }

    pub fn delete_session(&self, token: &str) -> bool {
        self.write().sessions.remove(token).is_some()
    }

    // -------------------------------------------------------- households --

    pub fn create_household(&self, name: String, currency: &str, owner: Uuid) -> Household {
        let hh = Household {
            id: Uuid::new_v4(),
            name,
            currency: currency.to_string(),
            created_at: OffsetDateTime::now_utc(),
        };
        let member = Membership {
            household_id: hh.id,
            user_id: owner,
            role: Role::Owner,
            status: MemberStatus::Active,
            created_at: OffsetDateTime::now_utc(),
        };
        let mut d = self.write();
        d.households.insert(hh.id, hh.clone());
        d.members.insert((hh.id, owner), member);
        hh
    }

    pub fn membership(&self, household: Uuid, user: Uuid) -> Option<Membership> {
        self.read().members.get(&(household, user)).cloned()
    }

    pub fn households_for_user(&self, user: Uuid) -> Vec<(Household, Role)> {
        let d = self.read();
        let mut list: Vec<(Household, Role)> = d
            .members
            .iter()
            .filter(|((hh, uid), m)| {
                *uid == user && m.status == MemberStatus::Active && d.households.contains_key(hh)
            })
            .map(|((hh, _), m)| (d.households[hh].clone(), m.role))
            .collect();
        list.sort_by(|a, b| a.0.created_at.cmp(&b.0.created_at));
        list
    }

    // ----------------------------------------------------------- accounts --

    pub fn insert_account(&self, account: Account) -> StoreResult<Account> {
        let mut d = self.write();
        if !d.households.contains_key(&account.household_id) {
            return Err(StoreError::NotFound("household"));
        }
        d.accounts.insert(account.id, account.clone());
        Ok(account)
    }

    /// Visibility rule: `AllMembers` → every active member; tighter scopes are
    /// visible to the creator and household owners only.
    pub fn can_view_account(&self, account: &Account, viewer: Uuid) -> bool {
        match account.visibility {
            Visibility::AllMembers => true,
            Visibility::PartnerOnly | Visibility::Private => {
                account.created_by == viewer || self.is_owner(account.household_id, viewer)
            }
        }
    }

    fn is_owner(&self, household: Uuid, user: Uuid) -> bool {
        matches!(
            self.membership(household, user),
            Some(Membership { role: Role::Owner, status: MemberStatus::Active, .. })
        )
    }

    pub fn get_account(&self, id: Uuid) -> Option<Account> {
        self.read().accounts.get(&id).cloned()
    }

    pub fn accounts_for_household(&self, household: Uuid, viewer: Uuid) -> Vec<Account> {
        let d = self.read();
        let owner = Self::role_is(d.members.get(&(household, viewer)), Role::Owner);
        let mut list: Vec<Account> = d
            .accounts
            .values()
            .filter(|a| match a.visibility {
                Visibility::AllMembers => true,
                Visibility::PartnerOnly | Visibility::Private => {
                    a.created_by == viewer || owner
                }
            })
            .filter(|a| a.household_id == household)
            .cloned()
            .collect();
        list.sort_by(|a, b| (a.created_at, a.id).cmp(&(b.created_at, b.id)));
        list
    }

    fn role_is(m: Option<&Membership>, role: Role) -> bool {
        matches!(m, Some(m) if m.role == role && m.status == MemberStatus::Active)
    }

    pub fn update_account(
        &self,
        id: Uuid,
        name: Option<String>,
        visibility: Option<Visibility>,
    ) -> StoreResult<Account> {
        let mut d = self.write();
        let a = d.accounts.get_mut(&id).ok_or(StoreError::NotFound("account"))?;
        if let Some(n) = name {
            a.name = n;
        }
        if let Some(v) = visibility {
            a.visibility = v;
        }
        Ok(a.clone())
    }

    pub fn delete_account(&self, id: Uuid) -> StoreResult<()> {
        let mut d = self.write();
        d.accounts.remove(&id).ok_or(StoreError::NotFound("account"))?;
        d.txns.retain(|_, t| t.account_id != id);
        Ok(())
    }

    // ------------------------------------------------------- transactions --

    /// Inserts a transaction and applies it to the running balance atomically.
    /// Synced accounts are provider-authoritative, so their balances never
    /// move locally — the ledger row is still recorded for analytics.
    pub fn create_transaction(&self, txn: Transaction) -> StoreResult<Transaction> {
        let mut d = self.write();
        let account_id = txn.account_id;
        let synced = d.accounts.get(&account_id).is_some_and(|a| a.connection_id.is_some());
        if !synced {
            if let Some(acc) = d.accounts.get_mut(&account_id) {
                acc.current_balance_minor += txn.amount_minor;
            }
        }
        d.txns.insert(txn.id, txn.clone());
        Ok(txn)
    }

    pub fn get_transaction(&self, id: Uuid) -> Option<Transaction> {
        self.read().txns.get(&id).cloned()
    }

    /// Patch fields; an amount change adjusts the balance of *manual*
    /// accounts by the delta (synced accounts defer to the provider).
    pub fn update_transaction(&self, id: Uuid, patch: TxPatch) -> StoreResult<Transaction> {
        let mut d = self.write();
        // Compute balance delta first so the txn borrow ends before the
        // account borrow begins (same-map two-phase update).
        let (updated, balance_delta) = {
            let txn = d.txns.get_mut(&id).ok_or(StoreError::NotFound("transaction"))?;
            let old_amount = txn.amount_minor;
            if let Some(new_amount) = patch.amount_minor {
                txn.amount_minor = new_amount;
            }
            if let Some(v) = patch.posted_on {
                txn.posted_on = v;
            }
            if let Some(v) = patch.merchant_raw {
                txn.merchant_raw = Some(v);
            }
            if let Some(v) = patch.description {
                txn.description = Some(v);
            }
            if let Some(v) = patch.notes {
                txn.notes = Some(v);
            }
            if let Some(v) = patch.review_state {
                txn.review_state = v;
            }
            (txn.clone(), txn.amount_minor - old_amount)
        };
        if balance_delta != 0
            && d.accounts.get(&updated.account_id).is_none_or(|a| a.connection_id.is_none())
            && let Some(acc) = d.accounts.get_mut(&updated.account_id)
        {
            acc.current_balance_minor += balance_delta;
        }
        Ok(updated)
    }

    /// Removes the transaction and reverses its effect on the balance
    /// (manual accounts only — synced balances belong to the provider).
    pub fn delete_transaction(&self, id: Uuid) -> StoreResult<()> {
        let mut d = self.write();
        let txn = d.txns.remove(&id).ok_or(StoreError::NotFound("transaction"))?;
        let synced = d.accounts.get(&txn.account_id).is_some_and(|a| a.connection_id.is_some());
        if !synced && let Some(acc) = d.accounts.get_mut(&txn.account_id) {
            acc.current_balance_minor -= txn.amount_minor;
        }
        Ok(())
    }

    // -------------------------------------------------------- connections --

    pub fn insert_connection(&self, conn: Connection) -> Connection {
        self.write().connections.insert(conn.id, conn.clone());
        conn
    }

    pub fn get_connection(&self, id: Uuid) -> Option<Connection> {
        self.read().connections.get(&id).cloned()
    }

    pub fn connection_by_item(&self, provider: &str, item_id: &str) -> Option<Connection> {
        self.read()
            .connections
            .values()
            .find(|c| c.provider == provider && c.external_item_id.as_deref() == Some(item_id))
            .cloned()
    }

    pub fn connections_for_household(&self, household: Uuid) -> Vec<Connection> {
        let mut list: Vec<Connection> = self
            .read()
            .connections
            .values()
            .filter(|c| c.household_id == household)
            .cloned()
            .collect();
        list.sort_by(|a, b| (a.created_at, a.id).cmp(&(b.created_at, b.id)));
        list
    }

    pub fn household_owner(&self, household: Uuid) -> Option<Uuid> {
        self.read()
            .members
            .iter()
            .find(|((hh, _), m)| {
                *hh == household && m.role == Role::Owner && m.status == MemberStatus::Active
            })
            .map(|((_, uid), _)| *uid)
    }

    /// System-context lookup (sync pipeline): ignores member visibility since
    /// synced accounts are household-wide by definition.
    pub fn accounts_for_connection(&self, connection: Uuid) -> Vec<Account> {
        let mut list: Vec<Account> = self
            .read()
            .accounts
            .values()
            .filter(|a| a.connection_id == Some(connection))
            .cloned()
            .collect();
        list.sort_by(|a, b| (a.created_at, a.id).cmp(&(b.created_at, b.id)));
        list
    }

    pub fn update_connection_status(
        &self,
        id: Uuid,
        status: ConnectionStatus,
        cursor: Option<String>,
    ) -> StoreResult<Connection> {
        let mut d = self.write();
        let c = d.connections.get_mut(&id).ok_or(StoreError::NotFound("connection"))?;
        c.status = status;
        if let Some(cur) = cursor {
            c.cursor = Some(cur);
        }
        Ok(c.clone())
    }

    /// Records a webhook event key; returns true only the first time it is
    /// seen — providers retry aggressively and every retry must be inert.
    pub fn claim_webhook_event(&self, provider: &str, event_key: &str) -> bool {
        let key = format!("{provider}:{event_key}");
        self.write().webhook_events.insert(key, ()).is_none()
    }

    // ------------------------------------------------ synced-entity upserts --

    /// Upsert by `(connection_id, external_id)`. Synced accounts are
    /// balance-authoritative: the provider's number always wins.
    pub fn upsert_synced_account(
        &self,
        household: Uuid,
        created_by: Uuid,
        connection: Uuid,
        ext: crate::aggregate::ExtAccount,
    ) -> Account {
        let mut d = self.write();
        let existing = d
            .accounts
            .values_mut()
            .find(|a| a.connection_id == Some(connection) && a.external_id.as_deref() == Some(ext.external_id.as_str()));
        if let Some(a) = existing {
            a.name = ext.name;
            a.account_type = ext.account_type;
            a.current_balance_minor = ext.current_balance_minor;
            return a.clone();
        }
        let account = Account {
            id: Uuid::new_v4(),
            household_id: household,
            connection_id: Some(connection),
            external_id: Some(ext.external_id),
            created_by,
            account_type: ext.account_type,
            name: ext.name,
            currency: ext.currency,
            current_balance_minor: ext.current_balance_minor,
            visibility: Visibility::AllMembers,
            created_at: OffsetDateTime::now_utc(),
        };
        d.accounts.insert(account.id, account.clone());
        account
    }

    /// Insert-if-new by `(account_id, external_id)`. Returns true when newly
    /// inserted. Balance adjustment applies to *manual* accounts only —
    /// synced accounts get their authoritative balance from the provider.
    pub fn upsert_synced_transaction(&self, txn: Transaction) -> bool {
        let mut d = self.write();
        let dup = txn.external_id.as_ref().is_some_and(|ext| {
            d.txns.values().any(|t| t.account_id == txn.account_id && t.external_id.as_deref() == Some(ext))
        });
        if dup {
            return false;
        }
        let synced = d
            .accounts
            .get(&txn.account_id)
            .is_some_and(|a| a.connection_id.is_some());
        if !synced && let Some(acc) = d.accounts.get_mut(&txn.account_id) {
            acc.current_balance_minor += txn.amount_minor;
        }
        d.txns.insert(txn.id, txn);
        true
    }

    /// Newest-first page ordered by `(posted_on, created_at)`; `before` is the
    /// previous page's final key for strict continuation.
    pub fn transactions_page(
        &self,
        household: Uuid,
        account: Option<Uuid>,
        before: Option<TxKey>,
        limit: usize,
    ) -> Vec<Transaction> {
        let d = self.read();
        let mut list: Vec<&Transaction> = d
            .txns
            .values()
            .filter(|t| t.household_id == household && account.is_none_or(|a| t.account_id == a))
            .collect();
        list.sort_by(|a, b| tx_key(b).cmp(&tx_key(a)));
        if let Some(key) = before {
            list.retain(|t| tx_key(t) < key);
        }
        list.into_iter().take(limit).cloned().collect()
    }
}

/// Convenience constructor used by handlers.
pub fn new_transaction(
    household: Uuid,
    account: Uuid,
    created_by: Uuid,
    posted_on: Date,
    amount_minor: i64,
    merchant_raw: Option<String>,
    description: Option<String>,
) -> Transaction {
    Transaction {
        id: Uuid::new_v4(),
        household_id: household,
        account_id: account,
        external_id: None,
        posted_on,
        amount_minor,
        merchant_raw,
        description,
        notes: None,
        review_state: ReviewState::Unreviewed,
        source: TxSource::Manual,
        created_by,
        created_at: OffsetDateTime::now_utc(),
    }
}
