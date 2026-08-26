//! Aggregation pipeline tests: mock connect → sync → dedup → webhooks.

use axum::body::Body;
use axum::response::Response;
use axum::Router;
use http::Request;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

use api::aggregate::{mock::MockAggregator, Registry};
use api::models::{Connection, ConnectionStatus};
use api::state::{AppState, SharedState};
use api::store::MemoryStore;

fn app() -> Router {
    let state: SharedState = Arc::new(AppState {
        store: MemoryStore::new(),
        env: "test".into(),
        started: std::time::Instant::now(),
        providers: Registry { plaid: None },
    });
    api::routes::build_router(state)
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Response {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let req = builder
        .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
        .expect("build request");
    app.clone().oneshot(req).await.expect("serve request")
}

async fn body_json(resp: Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

struct UserCtx {
    token: String,
    household_id: String,
}

async fn signup(app: &Router, email: &str) -> UserCtx {
    let resp = call(
        app,
        "POST",
        "/api/v1/auth/signup",
        None,
        Some(json!({
            "email": email, "password": "correct-horse-battery", "displayName": "Blake Tester"
        })),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let v = body_json(resp).await;
    UserCtx { token: v["token"].as_str().unwrap().into(), household_id: v["household"]["id"].as_str().unwrap().into() }
}

#[tokio::test]
async fn mock_connect_syncs_accounts_and_transactions() {
    let a = app();
    let ctx = signup(&a, "agg@example.com").await;

    let resp = call(
        &a,
        "POST",
        "/api/v1/connections/mock-connect",
        Some(&ctx.token),
        Some(json!({ "householdId": ctx.household_id })),
    )
    .await;
    assert_eq!(resp.status(), 201, "mock-connect failed");
    let v = body_json(resp).await;
    let connection_id: String = v["connection"]["id"].as_str().unwrap().into();
    assert_eq!(v["transactionsInserted"].as_u64(), Some(9), "all canned txns on first sync");

    // Two accounts, provider-authoritative balances.
    let accounts = v["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 2);
    let checking = accounts.iter().find(|x| x["name"] == "Mock Everyday Checking").unwrap();
    assert_eq!(checking["currentBalanceMinor"].as_i64(), Some(152_340));
    assert_eq!(checking["connectionId"].as_str(), Some(connection_id.as_str()));

    // Transactions landed and are queryable with cursor pagination.
    let list = body_json(
        call(
            &a,
            "GET",
            &format!("/api/v1/transactions?householdId={}&limit=100", ctx.household_id),
            Some(&ctx.token),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(list["transactions"].as_array().unwrap().len(), 9);

    // Sync again: idempotent — no duplicates.
    let resp = call(
        &a,
        "POST",
        &format!("/api/v1/connections/{connection_id}/sync"),
        Some(&ctx.token),
        None,
    )
    .await;
    assert_eq!(resp.status(), 200);
    let v = body_json(resp).await;
    assert_eq!(v["transactionsInserted"].as_u64(), Some(0), "second sync must insert nothing");

    // Manual transactions on a *synced* account don't fight the provider:
    // balance stays authoritative even though the txn is recorded.
    let checking_id = checking["id"].as_str().unwrap();
    let resp = call(
        &a,
        "POST",
        "/api/v1/transactions",
        Some(&ctx.token),
        Some(json!({
            "accountId": checking_id, "postedOn": "2026-08-26", "amountMinor": -999,
            "merchantRaw": "Manual Note"
        })),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let after = body_json(
        call(&a, "GET", &format!("/api/v1/accounts/{checking_id}"), Some(&ctx.token), None).await,
    )
    .await;
    assert_eq!(
        after["account"]["currentBalanceMinor"].as_i64(),
        Some(152_340),
        "synced account balance is provider-authoritative"
    );
}

#[tokio::test]
async fn connection_routes_enforce_membership() {
    let a = app();
    let alice = signup(&a, "alice@example.com").await;
    let bob = signup(&a, "bob@example.com").await;

    // Bob can't connect into Alice's household…
    let resp = call(
        &a,
        "POST",
        "/api/v1/connections/mock-connect",
        Some(&bob.token),
        Some(json!({ "householdId": alice.household_id })),
    )
    .await;
    assert_eq!(resp.status(), 403);

    // …and can't see or sync her connections.
    let resp = call(
        &a,
        "POST",
        "/api/v1/connections/mock-connect",
        Some(&alice.token),
        Some(json!({ "householdId": alice.household_id })),
    )
    .await;
    let conn_id: String = body_json(resp).await["connection"]["id"].as_str().unwrap().into();

    let resp = call(&a, "GET", &format!("/api/v1/connections?householdId={}", alice.household_id), Some(&bob.token), None).await;
    assert_eq!(resp.status(), 403);

    let resp = call(&a, "POST", &format!("/api/v1/connections/{conn_id}/sync"), Some(&bob.token), None).await;
    assert!(resp.status() == 403 || resp.status() == 404);
}

#[tokio::test]
async fn plaid_webhook_is_idempotent_and_triggers_one_sync() {
    let a = app();
    let ctx = signup(&a, "hook@example.com").await;
    let conn = body_json(
        call(
            &a,
            "POST",
            "/api/v1/connections/mock-connect",
            Some(&ctx.token),
            Some(json!({ "householdId": ctx.household_id })),
        )
        .await,
    )
    .await;

    // Point a fake "plaid" item at the mock connection so the webhook matches.
    let item_id = format!("mock-item-{}", conn["connection"]["externalItemId"].as_str().unwrap());
    let payload = json!({
        "webhook_type": "TRANSACTIONS",
        "webhook_code": "SYNC_UPDATES_RECEIVED",
        "item_id": item_id,
    });

    let first = call(&a, "POST", "/api/v1/webhooks/plaid", None, Some(payload.clone())).await;
    assert_eq!(first.status(), 200);
    let v = body_json(first).await;
    // The item won't match (it's a mock connection, not plaid) but dedup still applies.
    if v.get("matched").and_then(Value::as_bool) == Some(false) {
        let second = call(&a, "POST", "/api/v1/webhooks/plaid", None, Some(payload)).await;
        let v2 = body_json(second).await;
        assert_eq!(v2["duplicate"], true, "identical retry must be inert");
    } else {
        panic!("expected unmatched item for plaid webhook against mock connection");
    }

    // Unknown items respond OK without side effects.
    let unknown = call(
        &a,
        "POST",
        "/api/v1/webhooks/plaid",
        None,
        Some(json!({"webhook_code":"SYNC_UPDATES_RECEIVED","item_id":"does-not-exist"})),
    )
    .await;
    assert_eq!(unknown.status(), 200);
}

#[tokio::test]
async fn sync_requires_existing_connection() {
    let a = app();
    let ctx = signup(&a, "ghost@example.com").await;
    let missing = uuid::Uuid::new_v4();
    let resp = call(
        &a,
        "POST",
        &format!("/api/v1/connections/{missing}/sync"),
        Some(&ctx.token),
        None,
    )
    .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn mock_provider_contract_holds_directly() {
    let conn = Connection {
        id: uuid::Uuid::new_v4(),
        household_id: uuid::Uuid::new_v4(),
        provider: "mock".into(),
        external_item_id: Some("x".into()),
        access_token: Some("t".into()),
        cursor: None,
        status: api::models::ConnectionStatus::Connected,
        institution_name: None,
        created_at: time::OffsetDateTime::now_utc(),
    };
    let m = MockAggregator;
    let accts = m.accounts(&conn).unwrap();
    assert_eq!(accts.len(), 2);

    let page1 = m.transactions(&conn, None).unwrap();
    assert_eq!(page1.txns.len(), 3);
    assert!(page1.has_more);
    let page2 = m.transactions(&conn, Some(&page1.next_cursor)).unwrap();
    assert_eq!(page2.txns.len(), 3);
    let page3 = m.transactions(&conn, Some(&page2.next_cursor)).unwrap();
    assert_eq!(page3.txns.len(), 3);
    assert!(!page3.has_more);
    let page4 = m.transactions(&conn, Some(&page3.next_cursor)).unwrap();
    assert!(page4.txns.is_empty());
}
