//! Signup / login / logout / me.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{hash_password, verify_password, AuthUser, SESSION_TTL};
use crate::error::ApiError;
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignupReq {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

fn validate_email(email: &str) -> Result<(), ApiError> {
    let ok = matches!(email.split_once('@'), Some((local, dom)) if !local.is_empty() && dom.contains('.'));
    if ok {
        Ok(())
    } else {
        Err(ApiError::Validation("email must be name@domain.tld".into()))
    }
}

fn validate_signup(req: &SignupReq) -> Result<(), ApiError> {
    validate_email(&req.email)?;
    if req.password.len() < 8 {
        return Err(ApiError::Validation("password must be at least 8 characters".into()));
    }
    let name = req.display_name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::Validation("displayName must be 1–120 characters".into()));
    }
    Ok(())
}

fn token_response(
    state: &SharedState,
    user: crate::models::User,
) -> Result<axum::response::Response, ApiError> {
    let session = state.store.create_session(user.id, SESSION_TTL);
    Ok((StatusCode::OK, Json(json!({ "token": session.token, "user": user }))).into_response())
}

/// 201 with a session token + the auto-created starter household.
pub async fn signup(
    State(state): State<SharedState>,
    Json(req): Json<SignupReq>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    validate_signup(&req)?;
    let hash = hash_password(&req.password)?;
    let display_name = req.display_name.trim().to_string();
    let user = state.store.create_user(&req.email, hash, display_name.clone())?;
    let household_name = format!(
        "{}'s Household",
        display_name.split_whitespace().next().unwrap_or(&display_name)
    );
    let household = state.store.create_household(household_name, "USD", user.id);
    let session = state.store.create_session(user.id, SESSION_TTL);
    tracing::info!(user_id = %user.id, household_id = %household.id, "signup");
    Ok((
        StatusCode::CREATED,
        Json(json!({ "token": session.token, "user": user, "household": household })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct LoginReq {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(state): State<SharedState>,
    Json(req): Json<LoginReq>,
) -> Result<axum::response::Response, ApiError> {
    // Constant-ish work regardless of user existence (hash anyway) to blunt
    // account enumeration timing signals.
    let stored = state.store.user_by_email(&req.email);
    let valid = match &stored {
        Some(su) => verify_password(&req.password, &su.password_hash),
        None => {
            let _ = hash_password(&req.password);
            false
        }
    };
    if !valid {
        return Err(ApiError::Unauthorized);
    }
    token_response(&state, stored.unwrap().user)
}

pub async fn logout(
    State(state): State<SharedState>,
    auth: AuthUser,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if let Some(token) = bearer_token(&headers) {
        state.store.delete_session(token);
    }
    let _ = auth; // extractor already enforced authentication
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

pub async fn me(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let households: Vec<serde_json::Value> = state
        .store
        .households_for_user(auth.0.id)
        .into_iter()
        .map(|(hh, role)| json!({ "household": hh, "role": role }))
        .collect();
    Ok(Json(json!({ "user": auth.0, "households": households })))
}
