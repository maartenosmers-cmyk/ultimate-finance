//! End-to-end tests over the production router (in-memory store).

use axum::body::Body;
use axum::response::Response;
use axum::Router;
use http::Request;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

use api::state::{AppState, SharedState};
use api::store::MemoryStore;

fn app() -> Router {
    let state: SharedState = Arc::new(AppState {
        store: MemoryStore::new(),
        env: "test".into(),
        started: std::time::Instant::now(),
        providers: api::aggregate::Registry { plaid: None },
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
            "email": email,
            "password": "correct-horse-battery",
            "displayName": "Blake Tester"
        })),
    )
    .await;
    assert_eq!(resp.status(), 201, "signup failed: {:?}", resp.status());
    let v = body_json(resp).await;
    UserCtx {
        token: v["token"].as_str().unwrap().into(),
        household_id: v["household"]["id"].as_str().unwrap().into(),
    }
}

#[tokio::test]
async fn healthz_is_ok() {
    let resp = call(&app(), "GET", "/healthz", None, None).await;
    assert_eq!(resp.status(), 200);
    let v = body_json(resp).await;
    assert_eq!(v["status"], "ok");
}

#[tokio::test]
async fn signup_rejects_bad_input() {
    let a = app();
    for bad in [
        json!({"email": "nope", "password": "longenough1", "displayName": "X"}),
        json!({"email": "a@b.co", "password": "short", "displayName": "X"}),
        json!({"email": "a@b.co", "password": "longenough1", "displayName": "  "}),
        json!({"email": "", "password": "longenough1", "displayName": "X"}),
    ] {
        let resp = call(&a, "POST", "/api/v1/auth/signup", None, Some(bad)).await;
        assert_eq!(resp.status(), 422, "expected validation failure");
    }
}

#[tokio::test]
async fn signup_login_me_flow() {
    let a = app();
    let ctx = signup(&a, "flow@example.com").await;

    // Wrong password → 401.
    let resp = call(
        &a,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({"email": "flow@example.com", "password": "wrong-password"})),
    )
    .await;
    assert_eq!(resp.status(), 401);

    // Right password → token.
    let resp = call(
        &a,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({"email": "FLOW@example.com ", "password": "correct-horse-battery"})),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let login = body_json(resp).await;
    assert!(login["token"].as_str().is_some());

    // /me shows the starter household with owner role.
    let me = body_json(call(&a, "GET", "/api/v1/me", Some(&ctx.token), None).await).await;
    assert_eq!(me["user"]["email"], "flow@example.com");
    assert_eq!(me["households"][0]["household"]["id"], ctx.household_id.as_str());
    assert_eq!(me["households"][0]["role"], "owner");
}

#[tokio::test]
async fn duplicate_email_conflicts() {
    let a = app();
    signup(&a, "dupe@example.com").await;
    let resp = call(
        &a,
        "POST",
        "/api/v1/auth/signup",
        None,
        Some(json!({
            "email": "DUPE@example.com",
            "password": "different-pass-1",
            "displayName": "Impostor"
        })),
    )
    .await;
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn endpoints_require_auth() {
    let a = app();
    assert_eq!(call(&a, "GET", "/api/v1/accounts?householdId=00000000-0000-0000-0000-000000000000", None, None).await.status(), 401);
    assert_eq!(call(&a, "GET", "/api/v1/me", Some("garbage-token"), None).await.status(), 401);
}

/// Creates an account and returns its id.
async fn make_account(app: &Router, ctx: &UserCtx, name: &str, balance: i64) -> String {
    let resp = call(
        app,
        "POST",
        "/api/v1/accounts",
        Some(&ctx.token),
        Some(json!({
            "householdId": ctx.household_id,
            "type": "checking",
            "name": name,
            "currentBalanceMinor": balance
        })),
    )
    .await;
    assert_eq!(resp.status(), 201);
    body_json(resp).await["account"]["id"].as_str().unwrap().into()
}

#[tokio::test]
async fn account_crud_and_household_isolation() {
    let a = app();
    let alice = signup(&a, "alice@example.com").await;
    let bob = signup(&a, "bob@example.com").await;

    let acct_id = make_account(&a, &alice, "Everyday Checking", 500_000).await;

    // Owner sees it; stranger gets Forbidden on list and NotFound on direct get.
    let list = body_json(
        call(&a, "GET", &format!("/api/v1/accounts?householdId={}", alice.household_id), Some(&alice.token), None).await,
    ).await;
    assert_eq!(list["accounts"].as_array().unwrap().len(), 1);

    let foreign_list = call(
        &a,
        "GET",
        &format!("/api/v1/accounts?householdId={}", alice.household_id),
        Some(&bob.token),
        None,
    ).await;
    assert_eq!(foreign_list.status(), 403);

    let foreign_get = call(&a, "GET", &format!("/api/v1/accounts/{acct_id}"), Some(&bob.token), None).await;
    assert_eq!(foreign_get.status(), 403); // not a member of that household at all

    // Rename works.
    let resp = call(
        &a,
        "PATCH",
        &format!("/api/v1/accounts/{acct_id}"),
        Some(&alice.token),
        Some(json!({"name": "Primary Checking"})),
    ).await;
    assert_eq!(resp.status(), 200);
    let patched = body_json(resp).await;
    assert_eq!(patched["account"]["name"], "Primary Checking");

    // Delete cascades and returns 204; subsequent get is 404.
    let resp = call(&a, "DELETE", &format!("/api/v1/accounts/{acct_id}"), Some(&alice.token), None).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(call(&a, "GET", &format!("/api/v1/accounts/{acct_id}"), Some(&alice.token), None).await.status(), 404);
}

#[tokio::test]
async fn transactions_move_balances_exactly() {
    let a = app();
    let ctx = signup(&a, "ledger@example.com").await;
    let acct = make_account(&a, &ctx, "Wallet", 100_000).await; // $1,000

    // -$25 expense.
    let resp = call(
        &a,
        "POST",
        "/api/v1/transactions",
        Some(&ctx.token),
        Some(json!({
            "accountId": acct,
            "postedOn": "2026-08-20",
            "amountMinor": -2_500,
            "merchantRaw": "Coffee Shop"
        })),
    ).await;
    assert_eq!(resp.status(), 201);
    let txn_id: String = body_json(resp).await["transaction"]["id"].as_str().unwrap().into();

    let bal = |v: Value| v["account"]["currentBalanceMinor"].as_i64().unwrap();
    let get_acct =
        body_json(call(&a, "GET", &format!("/api/v1/accounts/{acct}"), Some(&ctx.token), None).await).await;
    assert_eq!(bal(get_acct), 97_500);

    // Zero amounts rejected.
    let resp = call(&a, "POST", "/api/v1/transactions", Some(&ctx.token),
        Some(json!({"accountId": acct, "postedOn": "2026-08-21", "amountMinor": 0}))).await;
    assert_eq!(resp.status(), 422);

    // Patch amount -2500 → -3000; balance follows the delta.
    let resp = call(&a, "PATCH", &format!("/api/v1/transactions/{txn_id}"), Some(&ctx.token),
        Some(json!({"amountMinor": -3_000, "reviewState": "reviewed"}))).await;
    assert_eq!(resp.status(), 200);
    let get_acct =
        body_json(call(&a, "GET", &format!("/api/v1/accounts/{acct}"), Some(&ctx.token), None).await).await;
    assert_eq!(bal(get_acct), 97_000);

    // Delete reverses fully back to opening balance.
    let resp = call(&a, "DELETE", &format!("/api/v1/transactions/{txn_id}"), Some(&ctx.token), None).await;
    assert_eq!(resp.status(), 204);
    let get_acct =
        body_json(call(&a, "GET", &format!("/api/v1/accounts/{acct}"), Some(&ctx.token), None).await).await;
    assert_eq!(bal(get_acct), 100_000);
}

#[tokio::test]
async fn pagination_walks_newest_first() {
    let a = app();
    let ctx = signup(&a, "pager@example.com").await;
    let acct = make_account(&a, &ctx, "Pager Card", 0).await;

    // Six transactions across ascending dates.
    for day in 1..=6u32 {
        let date = format!("2026-08-{day:02}");
        let resp = call(
            &a,
            "POST",
            "/api/v1/transactions",
            Some(&ctx.token),
            Some(json!({"accountId": acct, "postedOn": date, "amountMinor": -(day as i64) * 100})),
        ).await;
        assert_eq!(resp.status(), 201);
    }

    let urlencode =
        |s: &str| s.replace('/', "%2F").replace('"', "%22").replace('{', "%7B").replace('}', "%7D").replace(':', "%3A").replace(',', "%2C");
    let url = |before: Option<&str>| match before {
        Some(b) => format!("/api/v1/transactions?householdId={}&accountId={}&limit=4&before={}", ctx.household_id, acct, urlencode(b)),
        None => format!("/api/v1/transactions?householdId={}&accountId={}&limit=4", ctx.household_id, acct),
    };

    let page1 = body_json(call(&a, "GET", &url(None), Some(&ctx.token), None).await).await;
    let txns1 = page1["transactions"].as_array().unwrap();
    assert_eq!(txns1.len(), 4);
    // Newest first: day 06 → day 03.
    assert_eq!(txns1[0]["postedOn"], "2026-08-06");
    assert_eq!(txns1[3]["postedOn"], "2026-08-03");

    let cursor = page1["nextBefore"].as_str().unwrap().to_string();
    let page2 = body_json(call(&a, "GET", &url(Some(&cursor)), Some(&ctx.token), None).await).await;
    let txns2 = page2["transactions"].as_array().unwrap();
    assert_eq!(txns2.len(), 2);
    assert_eq!(txns2[0]["postedOn"], "2026-08-02");
    assert_eq!(txns2[1]["postedOn"], "2026-08-01");
    assert!(page2["nextBefore"].is_null(), "last page must not carry a cursor");
}

#[tokio::test]
async fn logout_invalidates_session() {
    let a = app();
    let ctx = signup(&a, "bye@example.com").await;
    let resp = call(&a, "POST", "/api/v1/auth/logout", Some(&ctx.token), None).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(call(&a, "GET", "/api/v1/me", Some(&ctx.token), None).await.status(), 401);
}
