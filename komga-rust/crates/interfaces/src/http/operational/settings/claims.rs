use axum::Json;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::operational_settings_access::claims as claims_access;

use super::super::super::OperationalState;

pub(crate) async fn get_claim_status(Extension(state): Extension<OperationalState>) -> Response {
    let is_claimed = claims_access::load_claim_status(state.runtime.database_file.as_path())
        .await
        .unwrap_or(false);

    Json(json!({ "isClaimed": is_claimed })).into_response()
}

pub(crate) async fn post_claim(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    let email = header_value(&headers, "x-komga-email");
    let password = header_value(&headers, "x-komga-password");
    let (Some(email), Some(password)) = (email, password) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    if claims_access::load_claim_status(state.runtime.database_file.as_path())
        .await
        .unwrap_or(false)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let hashed_password = match hash_bcrypt_password(password, DEFAULT_COST) {
        Ok(hash) => hash,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let created_user_id = generate_claimed_user_id();
    let created_user = match claims_access::claim_initial_admin_user(
        state.runtime.database_file.as_path(),
        &created_user_id,
        email.as_str(),
        hashed_password.as_str(),
    )
    .await
    {
        Ok(claims_access::ClaimInitialAdminUserResult::Created(created_user)) => created_user,
        Ok(claims_access::ClaimInitialAdminUserResult::AlreadyClaimed) => {
            return StatusCode::BAD_REQUEST.into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(json!({
        "id": created_user.id,
        "email": created_user.email,
        "roles": ["ADMIN"],
    }))
    .into_response()
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn generate_claimed_user_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    format!("rust-claim-{nanos:x}")
}
