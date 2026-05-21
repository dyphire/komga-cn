use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::operational::ClaimInitialAdminUserResult;

use crate::identity_access::auth::{AuthUser, user_payload_json};
use crate::state::OperationalApiState;

pub(crate) async fn get_claim_status(State(app): State<OperationalApiState>) -> Response {
    let is_claimed = app.claim.load_claim_status().await.unwrap_or(false);

    Json(json!({ "isClaimed": is_claimed })).into_response()
}

pub(crate) async fn post_claim(
    State(app): State<OperationalApiState>,
    headers: HeaderMap,
) -> Response {
    let email = email_header_value(&headers, "x-komga-email");
    let password = password_header_value(&headers, "x-komga-password");
    let (Some(email), Some(password)) = (email, password) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    if app.claim.load_claim_status().await.unwrap_or(false) {
        return claim_already_claimed_response();
    }

    let hashed_password = match hash_bcrypt_password(password, DEFAULT_COST) {
        Ok(hash) => hash,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let created_user_id = generate_claimed_user_id();
    let created_user = match app
        .claim
        .claim_initial_admin_user(&created_user_id, &email, &hashed_password)
        .await
    {
        Ok(ClaimInitialAdminUserResult::Created(created_user)) => created_user,
        Ok(ClaimInitialAdminUserResult::AlreadyClaimed) => {
            return claim_already_claimed_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let created_user = AuthUser {
        id: created_user.id,
        email: created_user.email,
        password: String::new(),
        roles: claim_user_roles()
            .iter()
            .map(|role| (*role).to_string())
            .collect(),
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    };

    Json(user_payload_json(&created_user)).into_response()
}

fn email_header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_claim_email(value))
        .map(str::to_string)
}

fn password_header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn valid_claim_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    let Some((domain_name, tld)) = domain.rsplit_once('.') else {
        return false;
    };
    !domain_name.is_empty() && !tld.is_empty()
}

fn claim_user_roles() -> &'static [&'static str] {
    &[
        "ADMIN",
        "FILE_DOWNLOAD",
        "PAGE_STREAMING",
        "KOBO_SYNC",
        "KOREADER_SYNC",
    ]
}

fn claim_already_claimed_response() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "Bad Request",
            "message": "This server has already been claimed",
            "path": "/api/v1/claim",
            "status": 400,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

fn generate_claimed_user_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    format!("rust-claim-{nanos:x}")
}
