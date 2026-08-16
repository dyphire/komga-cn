use axum::http::{HeaderMap, StatusCode};
use komga_application::identity_access::{AuthOutcome, AuthUser, PersistedApiKeyMetadata, user_id};

use crate::identity_access::auth::{
    persisted_api_key_metadata, persisted_api_key_user, persisted_api_key_user_by_token,
};
use crate::identity_access::device_auth::auth_resolvers::valid_kobo_path_token;
use crate::identity_access::device_auth::helpers::api_key_metadata_by_token;
use crate::media_assets::access_control::user_can_access_book_media;
use crate::state::{IdentityAccessState, IdentityState};

pub(super) async fn ensure_kobo_book_access(
    app: &IdentityAccessState,
    user: &AuthUser,
    book_id: &str,
) -> Result<(), StatusCode> {
    let media = app
        .book_media_reader
        .book_media(book_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    match user_can_access_book_media(app.book_media_reader.as_ref(), book_id, user, &media).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(StatusCode::FORBIDDEN),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub(super) async fn resolved_kobo_request_api_key_metadata(
    identity: &IdentityState,
    current_user: &AuthUser,
    auth_token: &str,
    headers: &HeaderMap,
) -> anyhow::Result<Option<PersistedApiKeyMetadata>> {
    if valid_kobo_path_token(auth_token) {
        match persisted_api_key_user_by_token(identity, auth_token).await {
            Ok(AuthOutcome::Valid(path_user)) if user_id(&path_user) == user_id(current_user) => {
                return api_key_metadata_by_token(identity, auth_token).await;
            }
            Ok(AuthOutcome::Valid(_) | AuthOutcome::Invalid | AuthOutcome::Missing) => {}
            Err(error) => return Err(error),
        }
    }

    let metadata = match persisted_api_key_user(identity, headers).await {
        Ok(AuthOutcome::Valid(header_user)) if user_id(&header_user) == user_id(current_user) => {
            persisted_api_key_metadata(identity, headers).await?
        }
        Ok(AuthOutcome::Valid(_) | AuthOutcome::Invalid | AuthOutcome::Missing) => None,
        Err(error) => return Err(error),
    };

    Ok(metadata)
}
