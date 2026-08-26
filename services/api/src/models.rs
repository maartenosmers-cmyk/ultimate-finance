//! Domain models. All money values are integer minor units (cents), signed:
//! income positive, expenses negative; liability balances carry a minus sign.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// `Date` ↔ `"YYYY-MM-DD"` string serde (time's built-in iso8601 module is
/// datetime-only).
pub mod serde_date {
    use super::*;
    use time::format_description::BorrowedFormatItem;
    use time::macros::format_description;

    const FORMAT: &[BorrowedFormatItem<'static>] = format_description!("[year]-[month]-[day]");

    pub fn serialize<S: serde::Serializer>(d: &time::Date, s: S) -> Result<S::Ok, S::Error> {
        let text = d.format(FORMAT).map_err(serde::ser::Error::custom)?;
        s.serialize_str(&text)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(de: D) -> Result<time::Date, D::Error> {
        let text = String::deserialize(de)?;
        time::Date::parse(&text, FORMAT).map_err(serde::de::Error::custom)
    }

    /// Same format for `Option<Date>` fields.
    pub mod option {
        use super::*;

        pub fn serialize<S: serde::Serializer>(
            d: &Option<time::Date>,
            s: S,
        ) -> Result<S::Ok, S::Error> {
            match d {
                Some(d) => super::serialize(d, s),
                None => s.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: serde::Deserializer<'de>>(
            de: D,
        ) -> Result<Option<time::Date>, D::Error> {
            let text = Option::<String>::deserialize(de)?;
            match text {
                Some(t) => time::Date::parse(&t, FORMAT).map(Some).map_err(serde::de::Error::custom),
                None => Ok(None),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct StoredUser {
    pub user: User,
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub token: String,
    pub user_id: Uuid,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Member,
    Advisor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Active,
    Invited,
    Revoked,
}

#[derive(Debug, Clone, Serialize)]
pub struct Membership {
    #[serde(rename = "householdId")]
    pub household_id: Uuid,
    #[serde(rename = "userId")]
    pub user_id: Uuid,
    pub role: Role,
    pub status: MemberStatus,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Household {
    pub id: Uuid,
    pub name: String,
    pub currency: String,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

// ------------------------------------------------------------- connections --

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Pending,
    Connected,
    RequiresReauth,
    Error,
    Disconnected,
}

/// One provider linkage (Plaid Item, or a mock) for a household.
#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    pub id: Uuid,
    #[serde(rename = "householdId")]
    pub household_id: Uuid,
    /// Aggregator name (`plaid`, `mock`).
    pub provider: String,
    #[serde(rename = "externalItemId")]
    pub external_item_id: Option<String>,
    /// Provider credential. In-memory store keeps it as-is; the Postgres
    /// migration stores this column KMS-encrypted (credentials_enc bytea).
    #[serde(skip_serializing)]
    pub access_token: Option<String>,
    /// Opaque sync cursor for incremental transaction pulls.
    pub cursor: Option<String>,
    pub status: ConnectionStatus,
    #[serde(rename = "institutionName")]
    pub institution_name: Option<String>,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Checking,
    Savings,
    #[serde(alias = "creditCard")]
    CreditCard,
    Brokerage,
    Retirement,
    Loan,
    Mortgage,
    Property,
    Vehicle,
    Cash,
    Other,
}

impl AccountType {
    /// Liability accounts hold negative balances.
    pub fn is_liability(self) -> bool {
        matches!(self, Self::CreditCard | Self::Loan | Self::Mortgage)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    AllMembers,
    PartnerOnly,
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub id: Uuid,
    #[serde(rename = "householdId")]
    pub household_id: Uuid,
    /// Set for aggregated accounts; null for manual ones.
    #[serde(rename = "connectionId")]
    pub connection_id: Option<Uuid>,
    /// Provider-side account id (e.g. Plaid account_id), unique per connection.
    #[serde(rename = "externalId")]
    pub external_id: Option<String>,
    #[serde(rename = "createdBy")]
    pub created_by: Uuid,
    #[serde(rename = "type")]
    pub account_type: AccountType,
    pub name: String,
    pub currency: String,
    /// Signed. Assets ≥ 0; credit cards / loans ≤ 0.
    #[serde(rename = "currentBalanceMinor")]
    pub current_balance_minor: i64,
    pub visibility: Visibility,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Unreviewed,
    Reviewed,
    NeedsAttention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxSource {
    Manual,
    Import,
    Aggregate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Transaction {
    pub id: Uuid,
    #[serde(rename = "householdId")]
    pub household_id: Uuid,
    #[serde(rename = "accountId")]
    pub account_id: Uuid,
    /// Provider-side transaction id, unique per account (dedup key for sync).
    #[serde(rename = "externalId")]
    pub external_id: Option<String>,
    /// Bank day, ISO-8601 (`2026-08-26`).
    #[serde(rename = "postedOn", with = "serde_date")]
    pub posted_on: time::Date,
    /// Signed: income positive, expense negative.
    #[serde(rename = "amountMinor")]
    pub amount_minor: i64,
    #[serde(rename = "merchantRaw")]
    pub merchant_raw: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    #[serde(rename = "reviewState")]
    pub review_state: ReviewState,
    pub source: TxSource,
    #[serde(rename = "createdBy")]
    pub created_by: Uuid,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
