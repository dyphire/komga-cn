use super::*;

pub(super) async fn resolved_kobo_request_api_key_metadata(
    current_user: &AuthUser,
    auth_token: &str,
    headers: &HeaderMap,
    database_file: &FsPath,
) -> Option<(String, String)> {
    if valid_kobo_path_token(auth_token)
        && let Some(AuthOutcome::Valid(path_user)) =
            persisted_api_key_user_by_token(auth_token, database_file).await
        && user_id(&path_user) == user_id(current_user)
    {
        return api_key_metadata_by_token(auth_token, database_file).await;
    }

    if let Some(AuthOutcome::Valid(header_user)) =
        persisted_api_key_user(headers, database_file).await
        && user_id(&header_user) == user_id(current_user)
    {
        return persisted_api_key_metadata(headers, database_file)
            .await
            .map(|metadata| (metadata.id, metadata.comment));
    }

    None
}
