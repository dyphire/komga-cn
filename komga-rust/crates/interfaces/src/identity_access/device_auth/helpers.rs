use super::*;
pub(super) use crate::identity_access::auth::{
    authentication_activity_headers_metadata_with_remote_addr, authentication_activity_write_input,
};

pub(super) async fn record_successful_api_key_authentication_by_token(
    identity: &IdentityState,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    user: &AuthUser,
    api_key: &str,
) -> Option<()> {
    let api_key_metadata = api_key_metadata_by_token(identity, api_key).await;
    let (api_key_id, api_key_comment) = api_key_metadata
        .as_ref()
        .map(|(id, comment)| (Some(id.as_str()), Some(comment.as_str())))
        .unwrap_or((None, None));

    persisted_record_successful_authentication_activity(
        identity,
        user,
        authentication_activity_write_input(
            &authentication_activity_headers_metadata_with_remote_addr(headers, remote_addr),
            "ApiKey",
            api_key_id,
            api_key_comment,
        ),
    )
    .await
}

pub(super) async fn api_key_metadata_by_token(
    identity: &IdentityState,
    api_key: &str,
) -> Option<(String, String)> {
    let mut metadata_headers = HeaderMap::new();
    metadata_headers.insert("x-api-key", HeaderValue::from_str(api_key).ok()?);
    persisted_api_key_metadata(identity, &metadata_headers)
        .await
        .map(|metadata| (metadata.id, metadata.comment))
}
