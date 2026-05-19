use super::*;

pub(super) async fn resolved_kobo_request_api_key_metadata(
    identity: &IdentityState,
    current_user: &AuthUser,
    auth_token: &str,
    headers: &HeaderMap,
) -> Option<(String, String)> {
    if valid_kobo_path_token(auth_token)
        && let Some(AuthOutcome::Valid(path_user)) =
            persisted_api_key_user_by_token(identity, auth_token).await
        && user_id(&path_user) == user_id(current_user)
    {
        return api_key_metadata_by_token(identity, auth_token).await;
    }

    if let Some(AuthOutcome::Valid(header_user)) = persisted_api_key_user(identity, headers).await
        && user_id(&header_user) == user_id(current_user)
    {
        return persisted_api_key_metadata(identity, headers)
            .await
            .map(|metadata| (metadata.id, metadata.comment));
    }

    None
}
