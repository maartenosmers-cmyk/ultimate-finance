//! Transaction CRUD + cursor pagination. Every mutation flows through the
//! store so balances stay consistent with the ledger.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::auth::{require_member, require_writer, AuthUser};
use crate::error::ApiError;
use crate::models::ReviewState;
use crate::routes::accounts::authorized_account;
use crate::state::SharedState;
use crate::store::{new_transaction, TxPatch};

const MAX_ABS_AMOUNT_MINOR: i64 = 100_000_000_000_000; // $1T guard rail
const DEFAULT_PAGE: usize = 50;
const MAX_PAGE: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTransactionReq {
    pub account_id: Uuid,
    #[serde(with = "crate::models::serde_date")]
    pub posted_on: Date,
    pub amount_minor: i64,
    pub merchant_raw: Option<String>,
    pub description: Option<String>,
}

fn validate_amount(amount: i64) -> Result<(), ApiError> {
    if amount == 0 {
        Err(ApiError::Validation("amountMinor must be non-zero (income positive, expense negative)".into()))
    } else if amount.abs() > MAX_ABS_AMOUNT_MINOR {
        Err(ApiError::Validation("amount out of range".into()))
    } else {
        Ok(())
    }
}

pub async fn create_transaction(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(req): Json<CreateTransactionReq>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    validate_amount(req.amount_minor)?;
    // Account existence + visibility; membership guard via its household.
    let account = authorized_account(&state, &auth, req.account_id)?;
    let access = require_member(&state, &auth.0, account.household_id)?;
    require_writer(&access)?;

    let txn = new_transaction(
        account.household_id,
        account.id,
        auth.0.id,
        req.posted_on,
        req.amount_minor,
        req.merchant_raw,
        req.description,
    );
    let saved = state.store.create_transaction(txn)?;
    tracing::info!(transaction_id = %saved.id, "transaction created");
    Ok((StatusCode::CREATED, Json(json!({ "transaction": saved }))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTxQuery {
    pub household_id: Uuid,
    pub account_id: Option<Uuid>,
    /// Opaque cursor from `nextBefore`.
    pub before: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct CursorKey<'a> {
    #[serde(rename = "postedOn", with = "crate::models::serde_date")]
    posted_on: &'a Date,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    created_at: &'a OffsetDateTime,
    id: &'a Uuid,
}

fn encode_cursor(t: &crate::models::Transaction) -> String {
    serde_json::to_string(&CursorKey {
        posted_on: &t.posted_on,
        created_at: &t.created_at,
        id: &t.id,
    })
    .unwrap_or_default()
}

fn decode_cursor(raw: &str) -> Option<(Date, OffsetDateTime, Uuid)> {
    let v: CursorKeyOwned = serde_json::from_str(raw).ok()?;
    Some((v.posted_on, v.created_at, v.id))
}

#[derive(Debug, Deserialize)]
struct CursorKeyOwned {
    #[serde(rename = "postedOn", with = "crate::models::serde_date")]
    posted_on: Date,
    #[serde(rename = "createdAt", with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    id: Uuid,
}

pub async fn list_transactions(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(q): Query<ListTxQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_member(&state, &auth.0, q.household_id)?;
    if let Some(account_id) = q.account_id {
        // Ensure the filtered account is visible to this viewer.
        authorized_account(&state, &auth, account_id)?;
    }
    let limit = q.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let before = q.before.as_deref().and_then(decode_cursor);
    let txns = state.store.transactions_page(q.household_id, q.account_id, before, limit);
    let next_before = if txns.len() == limit { txns.last().map(encode_cursor) } else { None };
    Ok(Json(json!({ "transactions": txns, "nextBefore": next_before })))
}

/// Load a transaction and confirm household membership.
fn authorized_transaction(
    state: &SharedState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<crate::models::Transaction, ApiError> {
    let txn = state.store.get_transaction(id).ok_or(ApiError::NotFound)?;
    require_member(state, &auth.0, txn.household_id)?;
    Ok(txn)
}

pub async fn get_transaction(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let txn = authorized_transaction(&state, &auth, id)?;
    Ok(Json(json!({ "transaction": txn })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchTransactionReq {
    pub amount_minor: Option<i64>,
    #[serde(default, with = "crate::models::serde_date::option")]
    pub posted_on: Option<Date>,
    pub merchant_raw: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub review_state: Option<ReviewState>,
}

pub async fn patch_transaction(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchTransactionReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let existing = authorized_transaction(&state, &auth, id)?;
    let access = require_member(&state, &auth.0, existing.household_id)?;
    require_writer(&access)?;
    if let Some(a) = req.amount_minor {
        validate_amount(a)?;
    }
    let updated = state.store.update_transaction(
        id,
        TxPatch {
            amount_minor: req.amount_minor,
            posted_on: req.posted_on,
            merchant_raw: req.merchant_raw,
            description: req.description,
            notes: req.notes,
            review_state: req.review_state,
        },
    )?;
    Ok(Json(json!({ "transaction": updated })))
}

pub async fn delete_transaction(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let existing = authorized_transaction(&state, &auth, id)?;
    let access = require_member(&state, &auth.0, existing.household_id)?;
    require_writer(&access)?;
    state.store.delete_transaction(id)?;
    Ok(StatusCode::NO_CONTENT)
}
