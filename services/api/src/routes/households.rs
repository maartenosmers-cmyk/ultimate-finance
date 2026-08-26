//! Household listing + creation. (Invites/partner joins land in M4.)

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::SharedState;

pub async fn list_households(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let households: Vec<serde_json::Value> = state
        .store
        .households_for_user(auth.0.id)
        .into_iter()
        .map(|(hh, role)| json!({ "household": hh, "role": role }))
        .collect();
    Ok(Json(json!({ "households": households })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateHouseholdReq {
    pub name: String,
}

pub async fn create_household(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(req): Json<CreateHouseholdReq>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::Validation("name must be 1–120 characters".into()));
    }
    let hh = state.store.create_household(name.to_string(), "USD", auth.0.id);
    Ok((axum::http::StatusCode::CREATED, Json(json!({ "household": hh }))))
}
