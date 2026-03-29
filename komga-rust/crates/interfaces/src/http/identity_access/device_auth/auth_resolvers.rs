use super::*;

pub(super) async fn resolved_kobo_user(
    auth_token: &str,
    headers: &HeaderMap,
    database_file: &FsPath,
) -> Option<AuthUser> {
    let token = auth_token.trim();
    let api_key_user = if token.is_empty() {
        Some(AuthOutcome::Missing)
    } else {
        persisted_api_key_user_by_token(token, database_file).await
    };

    resolve_kobo_user(api_key_user, auth_token_user(headers))
}

pub(super) fn resolved_koreader_user_id(headers: &HeaderMap) -> Option<String> {
    let auth_user = auth_token_user(headers);
    let header_user = headers
        .get("X-Auth-User")
        .or_else(|| headers.get("x-auth-user"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    resolve_koreader_user_id(
        auth_user.as_ref(),
        header_user.as_deref(),
        configured_api_key().as_deref(),
    )
}

pub(super) fn koreader_authorized(headers: &HeaderMap) -> bool {
    let auth_user = auth_token_user(headers);
    let header_user = headers
        .get("X-Auth-User")
        .or_else(|| headers.get("x-auth-user"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    komga_application::identity_access::koreader_authorized(
        auth_user.as_ref(),
        header_user.as_deref(),
        configured_api_key().as_deref(),
    )
}
