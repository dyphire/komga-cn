use super::*;

pub(super) async fn required_kobo_user(
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    database_file: &FsPath,
) -> Result<AuthUser, StatusCode> {
    if !valid_kobo_path_token(auth_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match persisted_api_key_user_by_token(auth_token, database_file).await {
        Some(AuthOutcome::Valid(user)) => {
            let _ = record_successful_api_key_authentication_by_token(
                headers,
                remote_addr,
                database_file,
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
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    database_file: &FsPath,
) -> Result<AuthUser, StatusCode> {
    if let Some(user) = presented_koreader_api_key_user(headers, remote_addr, database_file).await?
    {
        return if user_has_role(&user, "KOREADER_SYNC") {
            Ok(user)
        } else {
            Err(StatusCode::FORBIDDEN)
        };
    }

    session_user_with_role(headers, "KOREADER_SYNC")
}

pub(super) async fn required_koreader_user_id(
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    database_file: &FsPath,
) -> Result<String, StatusCode> {
    required_koreader_user(headers, remote_addr, database_file)
        .await
        .map(|user| user_id(&user).to_string())
}

pub(super) async fn presented_koreader_api_key_user(
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    database_file: &FsPath,
) -> Result<Option<AuthUser>, StatusCode> {
    let Some(header_user) = raw_koreader_header_user(headers) else {
        return Ok(None);
    };

    match persisted_api_key_user_by_token(header_user, database_file).await {
        Some(AuthOutcome::Valid(user)) => {
            let _ = record_successful_api_key_authentication_by_token(
                headers,
                remote_addr,
                database_file,
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
    headers: &HeaderMap,
    required_role: &str,
) -> Result<AuthUser, StatusCode> {
    match resolved_auth_user(headers) {
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
