use super::*;

pub async fn kobo_ping(
    Extension(auth_db): Extension<super::AuthDatabaseState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        auth_db.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    "pong".into_response()
}

pub async fn kobo_initialization(
    Extension(auth_db): Extension<super::AuthDatabaseState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        auth_db.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let base_url = request_base_url(&headers);
    let mut response = (
        StatusCode::OK,
        Json(json!({
            "Resources": {
                "device_auth": format!("/kobo/{auth_token}/v1/auth/device"),
                "library_sync": format!("/kobo/{auth_token}/v1/library/sync"),
                "image_host": base_url,
                "image_url_template": format!("/kobo/{auth_token}/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/false/image.jpg"),
                "image_url_quality_template": format!("/kobo/{auth_token}/v1/books/{{ImageId}}/thumbnail/{{Width}}/{{Height}}/{{Quality}}/{{IsGreyscale}}/image.jpg"),
            }
        })),
    )
        .into_response();
    let api_token = generated_kobo_api_token(auth_token.as_str(), user_id(&user));
    response.headers_mut().insert(
        HeaderName::from_static("x-kobo-apitoken"),
        HeaderValue::from_str(api_token.as_str()).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    response
}

pub async fn kobo_auth_device(
    Extension(auth_db): Extension<super::AuthDatabaseState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        auth_db.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let payload =
        serde_json::from_slice::<KoboDeviceAuthRequest>(&body).unwrap_or(KoboDeviceAuthRequest {
            user_key: String::new(),
        });
    let (access_token, refresh_token, tracking_id) =
        generated_kobo_token_triplet(payload.user_key.as_str());

    Json(KoboDeviceAuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        tracking_id,
        user_key: payload.user_key,
    })
    .into_response()
}
