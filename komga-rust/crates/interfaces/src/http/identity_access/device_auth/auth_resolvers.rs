use super::*;

pub(super) async fn resolved_kobo_user(
    auth_token: &str,
    headers: &HeaderMap,
    database_file: &FsPath,
) -> Option<AuthUser> {
    if valid_kobo_path_token(auth_token) {
        match persisted_api_key_user_by_token(auth_token, database_file).await {
            Some(AuthOutcome::Valid(user)) if user.roles.iter().any(|role| role == "KOBO_SYNC") => {
                return Some(*user);
            }
            _ => {}
        }
    }

    auth_token_user(headers)
}

pub(super) fn valid_kobo_path_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
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
