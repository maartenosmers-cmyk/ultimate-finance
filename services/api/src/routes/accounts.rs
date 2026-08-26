//! Account CRUD. Manual accounts only for now; aggregated accounts arrive
//! with the Plaid integration (M2) and reuse these handlers.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::{require_member, require_writer, AuthUser};
use crate::error::ApiError;
use crate::models::{Account, AccountType, Visibility};
use crate::state::SharedState;

const MAX_ABS_AMOUNT_MINOR: i64 = 100_000_000_000_000; // $1T guard rail

fn validate_currency(cur: &str) -> Result<(), ApiError> {
    let ok = cur.len() == 3 && cur.chars().all(|c| c.is_ascii_uppercase());
    if ok {
        Ok(())
    } else {
        Err(ApiError::Validation("currency must be 3 uppercase letters".into()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountReq {
    pub household_id: Uuid,
    #[serde(rename = "type")]
    pub account_type: AccountType,
    pub name: String,
    pub currency: Option<String>,
    pub current_balance_minor: Option<i64>,
    pub visibility: Option<Visibility>,
}

pub async fn create_account(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(req): Json<CreateAccountReq>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let access = require_member(&state, &auth.0, req.household_id)?;
    require_writer(&access)?;

    let name = req.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::Validation("name must be 1–120 characters".into()));
    }
    let currency = req.currency.unwrap_or_else(|| "USD".into());
    validate_currency(&currency)?;
    let balance = req.current_balance_minor.unwrap_or(0);
    if balance.abs() > MAX_ABS_AMOUNT_MINOR {
        return Err(ApiError::Validation("balance out of range".into()));
    }
    // Liability convention: opening balances on debt accounts must be ≤ 0.
    if req.account_type.is_liability() && balance > 0 && !matches!(req.account_type, AccountType::Other) {
        return Err(ApiError::Validation(
            "liability balances must be zero or negative (you owe less than the limit)".into(),
        ));
    }

    let account = Account {
        id: Uuid::new_v4(),
        household_id: req.household_id,
        connection_id: None,
        external_id: None,
        created_by: auth.0.id,
        account_type: req.account_type,
        name: name.to_string(),
        currency,
        current_balance_minor: balance,
        visibility: req.visibility.unwrap_or(Visibility::AllMembers),
        created_at: time::OffsetDateTime::now_utc(),
    };
    let saved = state.store.insert_account(account)?;
    tracing::info!(account_id = %saved.id, "account created");
    Ok((StatusCode::CREATED, Json(json!({ "account": saved }))))
}

#[derive(Debug, Deserialize)]
pub struct ListAccountsQuery {
    #[serde(rename = "householdId")]
    pub household_id: Uuid,
}

pub async fn list_accounts(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(q): Query<ListAccountsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_member(&state, &auth.0, q.household_id)?;
    let accounts = state.store.accounts_for_household(q.household_id, auth.0.id);
    Ok(Json(json!({ "accounts": accounts })))
}

/// Load + authorize a single account; invisible accounts look like missing ones.
pub(crate) fn authorized_account(
    state: &SharedState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<Account, ApiError> {
    let account = state.store.get_account(id).ok_or(ApiError::NotFound)?;
    require_member(state, &auth.0, account.household_id)?;
    if !state.store.can_view_account(&account, auth.0.id) {
        return Err(ApiError::NotFound);
    }
    Ok(account)
}

pub async fn get_account(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let account = authorized_account(&state, &auth, id)?;
    Ok(Json(json!({ "account": account })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchAccountReq {
    pub name: Option<String>,
    pub visibility: Option<Visibility>,
}

pub async fn patch_account(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchAccountReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let existing = authorized_account(&state, &auth, id)?;
    let access = require_member(&state, &auth.0, existing.household_id)?;
    require_writer(&access)?;
    if let Some(n) = &req.name {
        let n = n.trim();
        if n.is_empty() || n.len() > 120 {
            return Err(ApiError::Validation("name must be 1–120 characters".into()));
        }
    }
    let updated = state.store.update_account(id, req.name, req.visibility)?;
    Ok(Json(json!({ "account": updated })))
}

pub async fn delete_account(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let existing = authorized_account(&state, &auth, id)?;
    let access = require_member(&state, &auth.0, existing.household_id)?;
    require_writer(&access)?;
    state.store.delete_account(id)?;
    tracing::info!(account_id = %id, "account deleted");
    Ok(StatusCode::NO_CONTENT)
}
