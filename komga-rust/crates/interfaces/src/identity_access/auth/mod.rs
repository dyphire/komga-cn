mod extractors;
mod request_metadata;
mod response;
mod token;
mod user;

use axum::http::HeaderMap;

use crate::access_log;
use crate::state::IdentityState;

pub(crate) use extractors::{Admin, Authenticated, FileDownload};
pub(crate) use komga_application::identity_access::AuthenticationActivityApiKey;
use komga_application::identity_access::{AuthOutcome, AuthUser, user_id};
pub(crate) use request_metadata::{
    authentication_activity_headers_metadata_with_remote_addr,
    authentication_activity_request_metadata, authentication_activity_write_input,
};
pub(crate) use response::{
    bootstrap_api_key_user, bootstrap_user, bootstrap_user_with_remember_me_cookies,
    bootstrap_user_with_remember_me_token, expired_remember_me_cookie, expired_session_cookie,
    unauthorized_json_response,
};
pub(crate) use token::{
    auth_token_user, empty_auth_token_supplied, remember_me_requested,
    remember_me_token_from_headers, resolved_token, session_token_for_user_with_runtime_key,
    session_token_from_headers,
};
pub(crate) use user::{
    api_key_header_value, basic_credentials, persisted_api_key_comment_exists,
    persisted_api_key_metadata, persisted_api_key_user, persisted_api_key_user_by_token,
    persisted_basic_user, persisted_create_api_key, persisted_delete_api_key_by_id,
    persisted_list_api_keys, persisted_list_authentication_activity,
    persisted_record_successful_authentication_activity, persisted_update_password_by_user_id,
    persisted_users,
};

fn record_resolved_auth_user(auth_user: Option<AuthUser>) -> Option<AuthUser> {
    access_log::record_resolved_auth_user_id(auth_user.as_ref().map(user_id));
    auth_user
}

pub(crate) fn resolved_auth_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Result<Option<AuthUser>, String> {
    resolved_auth_token(identity, headers).map(|resolved| resolved.map(|resolved| resolved.user))
}

pub(crate) fn resolved_auth_token(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Result<Option<komga_application::identity_access::ResolvedAuthToken>, String> {
    let session_token = token::session_token_from_headers(headers);
    let remember_me_token = token::remember_me_token_from_headers(headers);
    let resolved = identity
        .session_resolver()
        .resolve_auth_token(session_token.as_deref(), remember_me_token.as_deref())?;
    access_log::record_resolved_auth_user_id(
        resolved.as_ref().map(|resolved| user_id(&resolved.user)),
    );
    Ok(resolved)
}

pub(crate) async fn resolved_request_auth_user(
    identity: &IdentityState,
    headers: &HeaderMap,
) -> Result<Option<AuthUser>, String> {
    let auth_user = match persisted_api_key_user(identity, headers).await {
        Ok(AuthOutcome::Valid(user)) => Some(*user),
        Ok(AuthOutcome::Invalid) => None,
        Ok(AuthOutcome::Missing) => match auth_token_user(identity, headers)? {
            Some(user) => Some(user),
            None => match persisted_basic_user(identity, headers).await {
                Ok(AuthOutcome::Valid(user)) => Some(*user),
                Ok(AuthOutcome::Invalid | AuthOutcome::Missing) => None,
                Err(error) => return Err(error),
            },
        },
        Err(error) => return Err(error),
    };

    Ok(record_resolved_auth_user(auth_user))
}

pub(crate) fn invalidate_user_sessions_for_runtime_key(
    identity: &IdentityState,
    user_id: &str,
    runtime_key: &str,
) {
    identity
        .session_lifecycle()
        .invalidate_user_sessions_with_runtime_key(user_id, runtime_key);
}
