//! Password hashing, session tokens, and the `AuthUser` extractor.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use std::sync::Arc;
use std::time::Duration;

use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use rand_core::{OsRng, RngCore};
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::User;
use crate::state::SharedState;

pub const SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 30); // 30 days

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| ApiError::Internal)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
        })
        .unwrap_or(false)
}

/// 32 random bytes as lowercase hex.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.try_fill_bytes(&mut bytes).expect("system RNG unavailable");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Authenticated caller, injected by the extractor below.
#[derive(Debug, Clone)]
pub struct AuthUser(pub User);

impl FromRequestParts<Arc<crate::state::AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<crate::state::AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthorized)?;

        let user = state
            .store
            .resolve_session(header.trim())
            .ok_or(ApiError::Unauthorized)?;
        Ok(AuthUser(user))
    }
}

/// Membership guard result for household-scoped handlers.
pub struct HouseholdAccess {
    pub role: crate::models::Role,
}

pub fn require_member(
    state: &SharedState,
    user: &User,
    household_id: Uuid,
) -> Result<HouseholdAccess, ApiError> {
    match state.store.membership(household_id, user.id) {
        Some(m) if m.status == crate::models::MemberStatus::Active => {
            Ok(HouseholdAccess { role: m.role })
        }
        // Distinguish "no such household" from "not your household"? Deliberately
        // not: both are Forbidden to avoid existence leaks.
        _ => Err(ApiError::Forbidden),
    }
}

/// Writers must be owner/member; advisors are read-only.
pub fn require_writer(access: &HouseholdAccess) -> Result<(), ApiError> {
    if access.role == crate::models::Role::Advisor {
        Err(ApiError::Forbidden)
    } else {
        Ok(())
    }
}
