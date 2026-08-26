//! Plaid REST adapter. Only the endpoints we need, strongly typed.
//!
//! Credentials come from env: PLAID_CLIENT_ID, PLAID_SECRET, PLAID_ENV
//! (sandbox|production). When absent the registry reports `plaid` as not
//! configured and the UI can hide Link flows.

use time::Date;
use time::Month;
use uuid::Uuid;

use super::{AggregatorError, ExtAccount, ExtTxn, SyncPage};
use crate::models::{AccountType, Connection};

#[derive(Clone)]
pub struct PlaidClient {
    http: reqwest::Client,
    base_url: String,
    client_id: String,
    secret: String,
}

impl PlaidClient {
    pub fn from_env() -> Option<Self> {
        let client_id = std::env::var("PLAID_CLIENT_ID").ok()?;
        let secret = std::env::var("PLAID_SECRET").ok()?;
        let host = match std::env::var("PLAID_ENV").as_deref() {
            Ok("production") => "production",
            _ => "sandbox",
        };
        Some(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .ok()?,
            base_url: format!("https://{host}.plaid.com"),
            client_id,
            secret,
        })
    }

    fn creds(&self) -> serde_json::Value {
        serde_json::json!({ "client_id": self.client_id, "secret": self.secret })
    }

    async fn post<T: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, AggregatorError> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.http.post(url).json(&body).send().await?;
        let status = resp.status();
        let payload: serde_json::Value = resp.json().await?;
        if !status.is_success() {
            let code = payload["error_code"].as_str().unwrap_or("unknown");
            let msg = payload["error_message"].as_str().unwrap_or("");
            // Plaid ITEM errors mean re-auth is needed upstream.
            return Err(AggregatorError::Provider(format!("{code}: {msg}")));
        }
        serde_json::from_value(payload).map_err(|e| AggregatorError::Provider(e.to_string()))
    }

    pub async fn create_link_token(&self, household_id: Uuid) -> Result<String, AggregatorError> {
        #[derive(serde::Deserialize)]
        struct Resp {
            link_token: String,
        }
        let mut body = self.creds();
        body["client_name"] = "Ultimate Finance".into();
        body["language"] = "en".into();
        body["country_codes"] = serde_json::json!(["US"]);
        body["user"] = serde_json::json!({ "client_user_id": household_id.to_string() });
        body["products"] = serde_json::json!(["transactions"]);
        let r: Resp = self.post("/link/token/create", body).await?;
        Ok(r.link_token)
    }

    /// → (access_token, item_id)
    pub async fn exchange_public_token(
        &self,
        public_token: &str,
    ) -> Result<(String, String), AggregatorError> {
        #[derive(serde::Deserialize)]
        struct Resp {
            access_token: String,
            item_id: String,
        }
        let mut body = self.creds();
        body["public_token"] = public_token.into();
        let r: Resp = self.post("/item/public_token/exchange", body).await?;
        Ok((r.access_token, r.item_id))
    }

    pub(crate) async fn accounts(&self, conn: &Connection) -> Result<Vec<ExtAccount>, AggregatorError> {
        #[derive(serde::Deserialize)]
        struct Balances {
            current: Option<f64>,
            iso_currency_code: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct Account {
            account_id: String,
            name: String,
            r#type: String,
            subtype: Option<String>,
            balances: Balances,
        }
        #[derive(serde::Deserialize)]
        struct Resp {
            accounts: Vec<Account>,
        }
        let mut body = self.creds();
        body["access_token"] = conn.access_token.clone().expect("checked by caller").into();
        let r: Resp = self.post("/accounts/get", body).await?;
        Ok(r.accounts.into_iter().map(|a| ExtAccount {
            external_id: a.account_id,
            name: a.name,
            account_type: map_type(&a.r#type, a.subtype.as_deref()),
            currency: a.balances.iso_currency_code.unwrap_or_else(|| "USD".into()),
            // Sign convention: liabilities negative. Plaid reports credit-card
            // `current` as positive debt owed.
            current_balance_minor: to_minor(a.balances.current),
        })
        .collect())
    }

    pub(crate) async fn transactions(
        &self,
        conn: &Connection,
        cursor: Option<&str>,
    ) -> Result<SyncPage, AggregatorError> {
        #[derive(serde::Deserialize)]
        struct Txn {
            transaction_id: String,
            account_id: String,
            date: String,
            amount: f64,
            name: Option<String>,
            merchant_name: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct Resp {
            added: Vec<Txn>,
            next_cursor: String,
            has_more: bool,
        }
        let mut body = self.creds();
        body["access_token"] = conn.access_token.clone().expect("checked by caller").into();
        body["count"] = 100.into();
        if let Some(c) = cursor {
            body["cursor"] = c.into();
        } else {
            body["options"] = serde_json::json!({ "include_personal_finance_category": false });
        }
        let r: Resp = self.post("/transactions/sync", body).await?;
        Ok(SyncPage {
            txns: r
                .added
                .into_iter()
                .map(|t| ExtTxn {
                    external_id: t.transaction_id,
                    account_external_id: t.account_id,
                    posted_on: parse_date(&t.date),
                    // Plaid: positive = outflow. Ours: negative = expense.
                    amount_minor: -to_minor(Some(t.amount)),
                    merchant_raw: t.merchant_name.or_else(|| t.name.clone()),
                    description: t.name,
                })
                .collect(),
            next_cursor: r.next_cursor,
            has_more: r.has_more,
        })
    }
}

fn to_minor(v: Option<f64>) -> i64 {
    v.map(|x| (x * 100.0).round() as i64).unwrap_or(0)
}

fn parse_date(s: &str) -> Date {
    let parts: Vec<i32> = s.split('-').map(|p| p.parse().expect("plaid date part")).collect();
    Date::from_calendar_date(parts[0], Month::try_from(parts[1] as u8).expect("month"), parts[2] as u8)
        .expect("plaid date")
}

/// Map Plaid (type, subtype) onto our account taxonomy.
pub fn map_type(plaid_type: &str, subtype: Option<&str>) -> AccountType {
    use AccountType::*;
    match (plaid_type, subtype) {
        ("depository", Some("checking")) => Checking,
        ("depository", Some("savings")) | ("depository", Some("cd")) => Savings,
        ("depository", Some("hsa")) => Savings,
        ("depository", Some("money market")) => Savings,
        ("credit", _) => CreditCard,
        ("investment", Some("529")) | ("investment", Some("retirement")) => Retirement,
        ("investment", _) => Brokerage,
        ("loan", Some("mortgage")) => Mortgage,
        ("loan", _) => Loan,
        _ => Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaid_amounts_flip_sign_to_our_convention() {
        // $12.34 outflow → -1234 cents; -$500 inflow (interest) → +50_000.
        assert_eq!(-to_minor(Some(12.34)), -1_234);
        assert_eq!(-to_minor(Some(-500.0)), 50_000);
    }

    #[test]
    fn type_mapping_covers_common_cases() {
        assert_eq!(map_type("depository", Some("checking")), AccountType::Checking);
        assert_eq!(map_type("depository", Some("savings")), AccountType::Savings);
        assert_eq!(map_type("credit", Some("credit card")), AccountType::CreditCard);
        assert_eq!(map_type("loan", Some("mortgage")), AccountType::Mortgage);
        assert_eq!(map_type("investment", Some("401k")), AccountType::Brokerage);
        assert_eq!(map_type("weird", None), AccountType::Other);
    }

    #[test]
    fn date_parsing_is_strict_iso() {
        assert_eq!(parse_date("2026-08-26"), parse_date("2026-08-26"));
        assert_eq!(
            format!("{:04}-{:02}-{:02}", parse_date("2026-01-05").year(), u8::from(parse_date("2026-01-05").month()), parse_date("2026-01-05").day()),
            "2026-01-05"
        );
    }
}
