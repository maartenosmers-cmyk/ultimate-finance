//! API error type → uniform JSON problem responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("internal error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "sign in required".into()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "no access to this resource".into()),
            ApiError::Validation(m) => (StatusCode::UNPROCESSABLE_ENTITY, "validation", m.clone()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            ApiError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", "something went wrong".into())
            }
        };
        (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
    }
}

impl From<crate::store::StoreError> for ApiError {
    fn from(e: crate::store::StoreError) -> Self {
        match e {
            crate::store::StoreError::EmailTaken => ApiError::Conflict("email already registered".into()),
            crate::store::StoreError::NotFound(what) => {
                tracing::warn!(what, "store NotFound mapped to 404");
                ApiError::NotFound
            }
        }
    }
}
