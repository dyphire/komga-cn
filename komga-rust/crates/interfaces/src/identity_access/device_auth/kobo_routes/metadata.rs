use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use super::{
    ensure_kobo_book_access, proxied_missing_kobo_book_response,
    wire::{KoboBookMetadataWire, build_kobo_book_metadata_payload},
};
use crate::access_log::RequestConnectionInfo;
use crate::identity_access::device_auth::auth_resolvers::required_kobo_user;
use crate::identity_access::device_auth::kobo_request_base_url;
use crate::state::IdentityAccessState;

pub(crate) async fn kobo_library_book_metadata(
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };

    let device_sync = app.identity.device_sync();
    let metadata = match device_sync.load_kobo_metadata_record(&book_id).await {
        Ok(Some(metadata)) => metadata,
        Ok(None) => {
            let book_exists = match device_sync.persisted_book_exists(&book_id).await {
                Ok(exists) => exists,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            if !book_exists {
                let proxy_path = format!("/v1/library/{book_id}/metadata");
                match proxied_missing_kobo_book_response(
                    app.server_settings.as_ref(),
                    app.identity.kobo_proxy(),
                    &Method::GET,
                    proxy_path.as_str(),
                    uri.query(),
                    &headers,
                    &Bytes::new(),
                )
                .await
                {
                    Ok(Some(response)) => return response,
                    Ok(None) => {}
                    Err(status) => return status.into_response(),
                }
            }
            return Json(Vec::<KoboBookMetadataWire>::new()).into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if let Err(status) = ensure_kobo_book_access(&app, &current_user, &book_id).await {
        return status.into_response();
    }

    let base_url = match kobo_request_base_url(
        app.server_settings.as_ref(),
        &app.operational.runtime,
        &headers,
    )
    .await
    {
        Ok(base_url) => base_url,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    Json(build_kobo_book_metadata_payload(
        &book_id,
        &metadata,
        base_url.as_str(),
        auth_token.as_str(),
    ))
    .into_response()
}
