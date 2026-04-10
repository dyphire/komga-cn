use super::*;
use crate::media_assets_runtime_access::{
    decode_epub_positions, load_persisted_epub_extension_blob,
};

fn has_koreader_user_create_auth_headers(headers: &HeaderMap) -> bool {
    headers.contains_key("X-Auth-User") || headers.contains_key("x-auth-user")
}

pub async fn koreader_user_create(headers: HeaderMap) -> Response {
    if has_koreader_user_create_auth_headers(&headers) && !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    (StatusCode::FORBIDDEN, "User creation is disabled").into_response()
}

pub async fn koreader_user_auth(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let header_user = headers
        .get("X-Auth-User")
        .or_else(|| headers.get("x-auth-user"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let Some(header_user) = header_user else {
        return StatusCode::FORBIDDEN.into_response();
    };

    let Some(AuthOutcome::Valid(user)) =
        persisted_api_key_user_by_token(header_user, auth_db.database_file.as_path()).await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let _ = record_successful_api_key_authentication_by_token(
        &headers,
        connection_info.remote_addr(),
        auth_db.database_file.as_path(),
        &user,
        header_user,
    )
    .await;

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.koreader.v1+json"),
        )],
        Json(json!({ "authorized": "OK" })),
    )
        .into_response()
}

pub async fn koreader_get_progress(
    Extension(state): Extension<OperationalState>,
    Path(book_hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(user_id_value) = resolved_koreader_user_id(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let target =
        match load_koreader_book_target(state.runtime.database_file.as_path(), &book_hash).await {
            Ok(Some(target)) => target,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(KoreaderBookLookupError::Conflict) => {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({ "error": "More than 1 book found with the same hash" })),
                )
                    .into_response();
            }
            Err(KoreaderBookLookupError::Persistence) => {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let Some(progress) = (match load_read_progress(
        state.runtime.database_file.as_path(),
        &target.id,
        &user_id_value,
    )
    .await
    {
        Ok(progress) => progress,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/vnd.koreader.v1+json"),
                ),
                (
                    HeaderName::from_static("x-reason"),
                    HeaderValue::from_static("No progress found for this book"),
                ),
            ],
        )
            .into_response();
    };

    let locator = parse_locator_payload(progress.locator.as_deref());
    let percentage = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            (progress.page.max(0) as f64 / target.page_count.max(1) as f64).clamp(0.0, 1.0)
        });
    let progress_value = match koreader_epub_progress_value(
        state.runtime.database_file.as_path(),
        &target.id,
        &locator,
    )
    .await
    {
        Some(progress_value) => progress_value,
        None => locator
            .get("koreaderProgress")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| progress.page.max(0).to_string()),
    };

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.koreader.v1+json"),
        )],
        Json(KoreaderProgressPayload {
            document: book_hash,
            percentage,
            progress: progress_value,
            device: progress.device_name,
            device_id: progress.device_id,
        }),
    )
        .into_response()
}

async fn koreader_epub_progress_value(
    database_file: &FsPath,
    book_id: &str,
    locator: &Value,
) -> Option<String> {
    let href = locator.get("href").and_then(Value::as_str)?.trim();
    if href.is_empty() {
        return None;
    }

    let (_extension_class, blob) = load_persisted_epub_extension_blob(database_file, book_id)
        .await
        .ok()??;
    let positions = decode_epub_positions(&blob).ok()?;
    let unique_hrefs = dedup_epub_hrefs(&positions);

    unique_hrefs
        .iter()
        .position(|value| value == href)
        .map(|index| format!("/body/DocFragment[{}].0", index + 1))
}

fn dedup_epub_hrefs(positions: &[Value]) -> Vec<String> {
    let mut unique_hrefs = Vec::<String>::new();
    for position in positions {
        let Some(position_href) = position.get("href").and_then(Value::as_str) else {
            continue;
        };
        let position_href = position_href.trim();
        if position_href.is_empty() || unique_hrefs.iter().any(|value| value == position_href) {
            continue;
        }
        unique_hrefs.push(position_href.to_string());
    }
    unique_hrefs
}

pub async fn koreader_put_progress(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<KoreaderProgressPayload>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let Some(user_id_value) = resolved_koreader_user_id(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let target = match load_koreader_book_target(
        state.runtime.database_file.as_path(),
        payload.document.as_str(),
    )
    .await
    {
        Ok(Some(target)) => target,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(KoreaderBookLookupError::Conflict) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "More than 1 book found with the same hash" })),
            )
                .into_response();
        }
        Err(KoreaderBookLookupError::Persistence) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let percentage = payload.percentage.clamp(0.0, 1.0);
    let completed = percentage >= 1.0;

    let (page, locator) =
        match load_persisted_epub_extension_blob(state.runtime.database_file.as_path(), &target.id)
            .await
        {
            Ok(Some((_extension_class, blob))) => {
                let Ok(positions) = decode_epub_positions(&blob) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                let unique_hrefs = dedup_epub_hrefs(&positions);
                let Some(resource_index) =
                    parse_koreader_epub_resource_index(payload.progress.as_str())
                else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                let Some(href) = unique_hrefs.get(resource_index) else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                let page = (resource_index as i64 + 1).clamp(1, target.page_count.max(1) as i64);
                (
                    page,
                    json!({
                        "href": href,
                        "type": "application/xhtml+xml",
                        "locations": {
                            "position": page,
                            "progression": 0.0,
                            "totalProgression": percentage,
                        },
                    }),
                )
            }
            Ok(None) => {
                let Some(page) = parse_koreader_progress_page(
                    payload.progress.as_str(),
                    target.page_count,
                    percentage,
                )
                .map(|value| value as i64) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                if !(1..=target.page_count.max(1) as i64).contains(&page) {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
                (
                    page,
                    json!({
                        "koreaderProgress": payload.progress,
                        "locations": {
                            "position": page,
                            "totalProgression": percentage,
                        },
                    }),
                )
            }
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

    if persist_read_progress_with_locator(
        state.runtime.database_file.as_path(),
        &target.id,
        &user_id_value,
        page,
        completed,
        payload.device_id.as_str(),
        payload.device.as_str(),
        now_sync_marker().as_str(),
        Some(locator),
    )
    .await
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}
