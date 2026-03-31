use super::user_models::{AuthOutcome, AuthUser, user_id};

pub fn resolve_kobo_user(
    api_key_user: Option<AuthOutcome>,
    session_user: Option<AuthUser>,
) -> Option<AuthUser> {
    if let Some(AuthOutcome::Valid(user)) = api_key_user {
        Some(*user)
    } else {
        session_user
    }
}

pub fn resolve_koreader_user_id(
    session_user: Option<&AuthUser>,
    header_user: Option<&str>,
    configured_api_key: Option<&str>,
) -> Option<String> {
    if let Some(user) = session_user {
        return Some(user_id(user).to_string());
    }

    match_configured_api_key(header_user, configured_api_key)
        .then(|| "koreader-api-key".to_string())
}

pub fn koreader_authorized(
    session_user: Option<&AuthUser>,
    header_user: Option<&str>,
    configured_api_key: Option<&str>,
) -> bool {
    session_user.is_some() || match_configured_api_key(header_user, configured_api_key)
}

pub fn configured_api_key_identity(
    presented_token: &str,
    configured_api_key: Option<&str>,
    configured_api_key_id: Option<&str>,
    configured_api_key_comment: Option<&str>,
) -> (String, String) {
    if configured_api_key.is_some_and(|value| value == presented_token) {
        (
            configured_api_key_id.unwrap_or("unknown").to_string(),
            configured_api_key_comment.unwrap_or("unknown").to_string(),
        )
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

fn match_configured_api_key(header_user: Option<&str>, configured_api_key: Option<&str>) -> bool {
    let Some(header_user) = header_user.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    configured_api_key.is_some_and(|api_key| header_user == api_key)
}
