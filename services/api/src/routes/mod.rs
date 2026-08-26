//! HTTP layer: handlers + router assembly.

mod accounts;
mod auth_routes;
mod connections;
mod households;
mod transactions;
mod webhooks;

use axum::routing::{get, post};
use axum::Router;

use crate::state::{SharedState};

/// All routes live here so tests exercise exactly what production serves.
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/healthz", get(health))
        .route("/api/v1/auth/signup", post(auth_routes::signup))
        .route("/api/v1/auth/login", post(auth_routes::login))
        .route("/api/v1/auth/logout", post(auth_routes::logout))
        .route("/api/v1/me", get(auth_routes::me))
        .route(
            "/api/v1/households",
            get(households::list_households).post(households::create_household),
        )
        .route(
            "/api/v1/accounts",
            get(accounts::list_accounts).post(accounts::create_account),
        )
        .route(
            "/api/v1/accounts/{id}",
            get(accounts::get_account)
                .patch(accounts::patch_account)
                .delete(accounts::delete_account),
        )
        .route(
            "/api/v1/transactions",
            get(transactions::list_transactions).post(transactions::create_transaction),
        )
        .route(
            "/api/v1/transactions/{id}",
            get(transactions::get_transaction)
                .patch(transactions::patch_transaction)
                .delete(transactions::delete_transaction),
        )
        .route(
            "/api/v1/connections",
            get(connections::list_connections),
        )
        .route("/api/v1/connections/link-token", post(connections::create_link_token))
        .route("/api/v1/connections/exchange", post(connections::exchange_public_token))
        .route("/api/v1/connections/mock-connect", post(connections::mock_connect))
        .route("/api/v1/connections/{id}/sync", post(connections::sync))
        .route("/api/v1/webhooks/plaid", post(webhooks::plaid))
        .with_state(state)
}

async fn root() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::OK,
        axum::Json(serde_json::json!({
            "service": "ultimate-finance-api",
            "health": "/healthz",
            "api": "/api/v1",
        })),
    )
}

async fn health(axum::extract::State(state): axum::extract::State<SharedState>) -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "env": state.env,
        "uptime_ms": state.started.elapsed().as_millis() as u64,
    }))
}
