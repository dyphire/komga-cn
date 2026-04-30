use super::*;
use axum::http::{HeaderMap, StatusCode};
use std::net::SocketAddr;

pub(super) async fn required_kobo_user(
    identity: &dyn IdentityService,
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<AuthUser, StatusCode> {
    if !valid_kobo_path_token(auth_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match persisted_api_key_user_by_token(identity, auth_token).await {
        Some(AuthOutcome::Valid(user)) => {
            let _ = record_successful_api_key_authentication_by_token(
                identity,
                headers,
                remote_addr,
                &user,
                auth_token,
            )
            .await;

            if user_has_role(&user, "KOBO_SYNC") {
                Ok(*user)
            } else {
                Err(StatusCode::FORBIDDEN)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

pub(super) fn valid_kobo_path_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(super) async fn required_koreader_user(
    identity: &dyn IdentityService,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<AuthUser, StatusCode> {
    if let Some(user) = presented_koreader_api_key_user(identity, headers, remote_addr).await? {
        return if user_has_role(&user, "KOREADER_SYNC") {
            Ok(user)
        } else {
            Err(StatusCode::FORBIDDEN)
        };
    }

    session_user_with_role(identity, headers, "KOREADER_SYNC")
}

pub(super) async fn required_koreader_user_id(
    identity: &dyn IdentityService,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<String, StatusCode> {
    required_koreader_user(identity, headers, remote_addr)
        .await
        .map(|user| user_id(&user).to_string())
}

pub(super) async fn presented_koreader_api_key_user(
    identity: &dyn IdentityService,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<Option<AuthUser>, StatusCode> {
    let Some(header_user) = raw_koreader_header_user(headers) else {
        return Ok(None);
    };

    match persisted_api_key_user_by_token(identity, header_user).await {
        Some(AuthOutcome::Valid(user)) => {
            let _ = record_successful_api_key_authentication_by_token(
                identity,
                headers,
                remote_addr,
                &user,
                header_user,
            )
            .await;
            Ok(Some(*user))
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn session_user_with_role(
    identity: &dyn IdentityService,
    headers: &HeaderMap,
    required_role: &str,
) -> Result<AuthUser, StatusCode> {
    match resolved_auth_user(identity, headers) {
        Some(user) if user_has_role(&user, required_role) => Ok(user),
        Some(_) => Err(StatusCode::FORBIDDEN),
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

pub(super) fn raw_koreader_header_user(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("X-Auth-User")
        .or_else(|| headers.get("x-auth-user"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
}
