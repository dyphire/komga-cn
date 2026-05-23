use super::*;

pub async fn kobo_library_book_metadata(
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    if let Err(status) = required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        return status.into_response();
    }

    let metadata = match load_kobo_metadata_record(&app, &book_id).await {
        Ok(Some(metadata)) => metadata,
        Ok(None) => {
            let book_exists = persisted_book_exists(&app, &book_id).await.unwrap_or(false);
            if !book_exists {
                let proxy_path = format!("/v1/library/{book_id}/metadata");
                if let Some(response) = proxied_missing_kobo_book_response(
                    &app,
                    &axum::http::Method::GET,
                    proxy_path.as_str(),
                    uri.query(),
                    &headers,
                    &Bytes::new(),
                )
                .await
                {
                    return response;
                }
            }
            return Json(Value::Array(Vec::new())).into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let base_url = kobo_request_base_url(&app, &headers).await;
    Json(build_kobo_book_metadata_payload(
        &book_id,
        &metadata,
        base_url.as_str(),
        auth_token.as_str(),
    ))
    .into_response()
}
