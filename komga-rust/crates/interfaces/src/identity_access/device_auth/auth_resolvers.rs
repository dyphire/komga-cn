use axum::http::{HeaderMap, StatusCode};
use komga_application::identity_access::{
    AuthOutcome, AuthUser, AuthUserRole, user_has_role, user_id,
};
use std::net::SocketAddr;

use crate::identity_access::auth::{persisted_api_key_user_by_token, resolved_auth_user};
use crate::identity_access::device_auth::helpers::record_successful_api_key_authentication_by_token;
use crate::state::IdentityState;

pub(super) async fn required_kobo_user(
    identity: &IdentityState,
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<AuthUser, StatusCode> {
    if !valid_kobo_path_token(auth_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match persisted_api_key_user_by_token(identity, auth_token).await {
        Ok(AuthOutcome::Valid(user)) => {
            let _ = record_successful_api_key_authentication_by_token(
                identity,
                headers,
                remote_addr,
                &user,
                auth_token,
            )
            .await;

            if user_has_role(&user, AuthUserRole::KoboSync) {
                Ok(*user)
            } else {
                Err(StatusCode::FORBIDDEN)
            }
        }
        Ok(AuthOutcome::Invalid | AuthOutcome::Missing) => Err(StatusCode::UNAUTHORIZED),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub(super) fn valid_kobo_path_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(super) async fn required_koreader_user(
    identity: &IdentityState,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<AuthUser, StatusCode> {
    if let Some(user) = presented_koreader_api_key_user(identity, headers, remote_addr).await? {
        return if user_has_role(&user, AuthUserRole::KoreaderSync) {
            Ok(user)
        } else {
            Err(StatusCode::FORBIDDEN)
        };
    }

    session_user_with_role(identity, headers, AuthUserRole::KoreaderSync)
}

pub(super) async fn required_koreader_user_id(
    identity: &IdentityState,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<String, StatusCode> {
    required_koreader_user(identity, headers, remote_addr)
        .await
        .map(|user| user_id(&user).to_string())
}

pub(super) async fn presented_koreader_api_key_user(
    identity: &IdentityState,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> Result<Option<AuthUser>, StatusCode> {
    let Some(header_user) = raw_koreader_header_user(headers) else {
        return Ok(None);
    };

    match persisted_api_key_user_by_token(identity, header_user).await {
        Ok(AuthOutcome::Valid(user)) => {
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
        Ok(AuthOutcome::Invalid | AuthOutcome::Missing) => Err(StatusCode::UNAUTHORIZED),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn session_user_with_role(
    identity: &IdentityState,
    headers: &HeaderMap,
    required_role: AuthUserRole,
) -> Result<AuthUser, StatusCode> {
    match resolved_auth_user(identity, headers) {
        Ok(Some(user)) if user_has_role(&user, required_role) => Ok(user),
        Ok(Some(_)) => Err(StatusCode::FORBIDDEN),
        Ok(None) => Err(StatusCode::UNAUTHORIZED),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub(super) fn raw_koreader_header_user(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("X-Auth-User")
        .or_else(|| headers.get("x-auth-user"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
}
