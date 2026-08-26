//! Inbound aggregator webhooks. Contract: idempotent, fast, never trust the
//! payload for state — treat it as a hint to pull.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use crate::aggregate;
use crate::error::ApiError;
use crate::models::ConnectionStatus;
use crate::state::SharedState;

/// Plaid posts `SYNC_UPDATES_RECEIVED`, `INITIAL_UPDATE`, etc. We verify
/// nothing yet (sandbox-friendly); production must enable JWT verification —
/// tracked in ROADMAP M2. Dedup is by body hash so provider retries are inert.
pub async fn plaid(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let item_id = body["item_id"].as_str().unwrap_or_default().to_string();
    let code = body["webhook_code"].as_str().unwrap_or_default().to_string();

    // Dedup key: stable hash of the raw body. Providers retry; only the first
    // delivery may trigger work.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    serde_json::to_string(&body).unwrap_or_default().hash(&mut hasher);
    let event_key = format!("{:016x}", hasher.finish());
    if !state.store.claim_webhook_event("plaid", &event_key) {
        tracing::info!(%event_key, "duplicate webhook ignored");
        return Ok((StatusCode::OK, Json(json!({ "duplicate": true }))));
    }

    let Some(conn) = state.store.connection_by_item("plaid", &item_id) else {
        return Ok((StatusCode::OK, Json(json!({ "received": true, "matched": false }))));
    };

    match code.as_str() {
        "SYNC_UPDATES_RECEIVED" | "INITIAL_UPDATE" | "HISTORICAL_UPDATE" => {
            let state_for_task = state.clone();
            tokio::spawn(async move {
                match aggregate::sync_connection(&state_for_task, &conn).await {
                    Ok(n) => tracing::info!(connection_id = %conn.id, inserted = n, "webhook sync"),
                    Err(e) => {
                        tracing::warn!(connection_id = %conn.id, error = %e, "webhook sync failed");
                        let _ = state_for_task.store.update_connection_status(
                            conn.id,
                            ConnectionStatus::Error,
                            None,
                        );
                    }
                }
            });
        }
        other => tracing::info!(code = %other, item_id = %item_id, "webhook noted"),
    }

    Ok((StatusCode::OK, Json(json!({ "received": true }))))
}
