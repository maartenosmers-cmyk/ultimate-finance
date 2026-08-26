//! Connection lifecycle: link-token, exchange, mock-connect (dev), list, sync.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::aggregate::{self, AggregatorError};
use crate::auth::{require_member, require_writer, AuthUser};
use crate::error::ApiError;
use crate::models::{Connection, ConnectionStatus};
use crate::state::SharedState;

fn agg_err(e: AggregatorError) -> ApiError {
    match e {
        AggregatorError::NotConfigured => {
            ApiError::Conflict("provider not configured on this server".into())
        }
        AggregatorError::Unsupported(m) => ApiError::Validation(m.to_string()),
        AggregatorError::Provider(m) => {
            tracing::warn!(%m, "aggregator error");
            ApiError::Conflict(format!("provider error: {m}"))
        }
        AggregatorError::Network(e) => {
            tracing::warn!(error = %e, "aggregator network error");
            ApiError::Internal
        }
        AggregatorError::Store(e) => e.into(),
    }
}

fn new_connection(household: Uuid, provider: &str) -> Connection {
    Connection {
        id: Uuid::new_v4(),
        household_id: household,
        provider: provider.into(),
        external_item_id: None,
        access_token: None,
        cursor: None,
        status: ConnectionStatus::Pending,
        institution_name: None,
        created_at: OffsetDateTime::now_utc(),
    }
}

async fn authorize_connection(
    state: &SharedState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<Connection, ApiError> {
    let conn = state.store.get_connection(id).ok_or(ApiError::NotFound)?;
    let access = require_member(state, &auth.0, conn.household_id)?;
    require_writer(&access)?;
    Ok(conn)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTokenReq {
    pub household_id: Uuid,
}

/// Step 1 of Plaid Link: mint a short-lived link token for the client SDK.
pub async fn create_link_token(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(req): Json<LinkTokenReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_member(&state, &auth.0, req.household_id)?;
    let provider = state.providers.get("plaid").map_err(agg_err)?;
    let token = provider.create_link_token(req.household_id).await.map_err(agg_err)?;
    Ok(Json(json!({ "linkToken": token })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeReq {
    pub household_id: Uuid,
    pub public_token: String,
}

/// Step 2: trade the public token from Link for a persistent connection.
pub async fn exchange_public_token(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(req): Json<ExchangeReq>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let access = require_member(&state, &auth.0, req.household_id)?;
    require_writer(&access)?;
    let provider = state.providers.get("plaid").map_err(agg_err)?;

    let mut conn = new_connection(req.household_id, "plaid");
    let (access_token, item_id) =
        provider.exchange_public_token(&req.public_token).await.map_err(agg_err)?;
    conn.access_token = Some(access_token);
    conn.external_item_id = Some(item_id);
    conn.status = ConnectionStatus::Connected;

    let saved = state.store.insert_connection(conn);
    aggregate::sync_connection(&state, &saved).await.map_err(agg_err)?;

    tracing::info!(connection_id = %saved.id, "plaid connection established");
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "connection": saved,
            "accounts": state.store.accounts_for_connection(saved.id),
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockConnectReq {
    pub household_id: Uuid,
}

/// Dev-only instant connection to the deterministic mock institution so the
/// full pipeline is usable without Plaid credentials.
pub async fn mock_connect(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(req): Json<MockConnectReq>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if state.env == "production" {
        return Err(ApiError::NotFound);
    }
    let access = require_member(&state, &auth.0, req.household_id)?;
    require_writer(&access)?;

    let mut conn = new_connection(req.household_id, "mock");
    conn.access_token = Some(Uuid::new_v4().to_string());
    conn.external_item_id = Some(format!("mock-item-{}", conn.id));
    conn.institution_name = Some("Mock Bank".into());
    conn.status = ConnectionStatus::Connected;
    let saved = state.store.insert_connection(conn);

    let inserted = aggregate::sync_connection(&state, &saved).await.map_err(agg_err)?;
    tracing::info!(connection_id = %saved.id, inserted, "mock connected + synced");
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "connection": saved,
            "accounts": state.store.accounts_for_connection(saved.id),
            "transactionsInserted": inserted,
        })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ListConnectionsQuery {
    #[serde(rename = "householdId")]
    pub household_id: Uuid,
}

pub async fn list_connections(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(q): Query<ListConnectionsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_member(&state, &auth.0, q.household_id)?;
    Ok(Json(json!({ "connections": state.store.connections_for_household(q.household_id) })))
}

/// Pull fresh data now. Also triggered by webhooks in production.
pub async fn sync(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = authorize_connection(&state, &auth, id).await?;
    let inserted = aggregate::sync_connection(&state, &conn).await.map_err(agg_err)?;
    let updated = state.store.get_connection(id).ok_or(ApiError::NotFound)?;
    Ok(Json(json!({ "connection": updated, "transactionsInserted": inserted })))
}
