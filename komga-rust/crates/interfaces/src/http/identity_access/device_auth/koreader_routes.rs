use super::*;

pub async fn koreader_user_create() -> Response {
    (StatusCode::FORBIDDEN, "User creation is disabled").into_response()
}

pub async fn koreader_user_auth(headers: HeaderMap) -> Response {
    if !koreader_authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

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
        return StatusCode::NOT_FOUND.into_response();
    };

    let locator = parse_locator_payload(progress.locator.as_deref());
    let percentage = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            (progress.page.max(0) as f64 / target.page_count.max(1) as f64).clamp(0.0, 1.0)
        });
    let progress_value = locator
        .get("koreaderProgress")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| progress.page.max(0).to_string());

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
    let page =
        parse_koreader_progress_page(payload.progress.as_str(), target.page_count, percentage)
            as i64;
    let completed = percentage >= 1.0;
    let locator = json!({
        "koreaderProgress": payload.progress,
        "locations": {
            "position": page,
            "totalProgression": percentage,
        },
    });

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
