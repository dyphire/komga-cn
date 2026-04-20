use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

const KOREADER_PROGRESS_PATH: &str = "/koreader/syncs/progress";
const KOREADER_PROGRESS_PATH_PREFIX: &str = "/koreader/syncs/progress/";

enum KoreaderMediaProfile {
    Visual,
    Epub,
}

fn koreader_auth_failure(status: StatusCode, header_user_presented: bool) -> Response {
    if status == StatusCode::UNAUTHORIZED && !header_user_presented {
        StatusCode::FORBIDDEN.into_response()
    } else {
        status.into_response()
    }
}

fn koreader_progress_error_response(status: StatusCode, message: &str, path: &str) -> Response {
    let reason = status.canonical_reason().unwrap_or("Error");
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.koreader.v1+json"),
        )],
        Json(json!({
            "error": reason,
            "message": message,
            "path": path,
            "status": status.as_u16(),
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

fn koreader_media_profile(media_type: &str) -> Option<KoreaderMediaProfile> {
    match media_type {
        "application/epub+zip" => Some(KoreaderMediaProfile::Epub),
        "application/pdf"
        | "application/zip"
        | "application/vnd.comicbook+zip"
        | "application/vnd.comicbook-rar"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => Some(KoreaderMediaProfile::Visual),
        value if value.starts_with("image/") => Some(KoreaderMediaProfile::Visual),
        _ => None,
    }
}

async fn load_koreader_book_target(
    app: &HttpAppState,
    book_hash: &str,
) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
    app.services
        .runtime_identity
        .load_koreader_book_target(
            app.operational.runtime.database_file.clone(),
            book_hash.to_string(),
        )
        .await
}

async fn load_read_progress(
    app: &HttpAppState,
    book_id: &str,
    user_id: &str,
) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
    app.services
        .runtime_identity
        .load_read_progress(
            app.operational.runtime.database_file.clone(),
            book_id.to_string(),
            user_id.to_string(),
        )
        .await
}

pub async fn koreader_user_create(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let header_user_presented = raw_koreader_header_user(&headers).is_some();
    if let Err(status) = required_koreader_user(
        &headers,
        connection_info.remote_addr(),
        app.auth_db.database_file.as_path(),
    )
    .await
    {
        return koreader_auth_failure(status, header_user_presented);
    }

    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "Forbidden",
            "message": "User creation is disabled",
            "path": "/koreader/users/create",
            "status": 403,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

pub async fn koreader_user_auth(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let header_user_presented = raw_koreader_header_user(&headers).is_some();
    match required_koreader_user(
        &headers,
        connection_info.remote_addr(),
        app.auth_db.database_file.as_path(),
    )
    .await
    {
        Ok(_) => {}
        Err(status) => return koreader_auth_failure(status, header_user_presented),
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
    Extension(app): Extension<HttpAppState>,
    Path(book_hash): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    let user_id_value = match required_koreader_user_id(
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        Ok(user_id_value) => user_id_value,
        Err(status) => return status.into_response(),
    };

    let target = match load_koreader_book_target(&app, &book_hash).await {
        Ok(Some(target)) => target,
        Ok(None) => {
            return koreader_progress_error_response(
                StatusCode::NOT_FOUND,
                "Book not found",
                &format!("{KOREADER_PROGRESS_PATH_PREFIX}{book_hash}"),
            );
        }
        Err(KoreaderBookLookupError::Conflict) => {
            return koreader_progress_error_response(
                StatusCode::CONFLICT,
                "More than 1 book found with the same hash",
                &format!("{KOREADER_PROGRESS_PATH_PREFIX}{book_hash}"),
            );
        }
        Err(KoreaderBookLookupError::Persistence) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Some(progress) = (match load_read_progress(&app, &target.id, &user_id_value).await {
        Ok(progress) => progress,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return koreader_progress_error_response(
            StatusCode::OK,
            "No progress found for this book",
            &format!("{KOREADER_PROGRESS_PATH_PREFIX}{book_hash}"),
        );
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
        &app,
        app.operational.runtime.database_file.as_path(),
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
    app: &HttpAppState,
    _database_file: &FsPath,
    book_id: &str,
    locator: &Value,
) -> Option<String> {
    let href = locator.get("href").and_then(Value::as_str)?.trim();
    if href.is_empty() {
        return None;
    }

    let (_extension_class, blob) = app
        .services
        .media_assets
        .load_persisted_epub_extension_blob(
            app.operational.runtime.database_file.clone(),
            book_id.to_string(),
        )
        .await
        .ok()??;
    let positions = app.services.media_assets.decode_epub_positions(blob).ok()?;
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

fn koreader_epub_locator(href: &str, matched_position: &Value) -> Value {
    let mut locator = json!({
        "href": href,
        "type": matched_position
            .get("type")
            .cloned()
            .unwrap_or_else(|| Value::String("application/xhtml+xml".to_string())),
        "locations": {
            "progression": 0.0,
            "totalProgression": matched_position
                .get("locations")
                .and_then(|value| value.get("totalProgression"))
                .cloned()
                .unwrap_or(Value::Null),
        },
    });

    if let Some(kobo_span) = matched_position.get("koboSpan").cloned()
        && !kobo_span.is_null()
    {
        locator
            .as_object_mut()
            .expect("koreader epub locator should be an object")
            .insert("koboSpan".to_string(), kobo_span);
    }

    locator
}

pub async fn koreader_put_progress(
    Extension(app): Extension<HttpAppState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let header_user_presented = raw_koreader_header_user(&headers).is_some();

    let user_id_value = match required_koreader_user_id(
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        Ok(user_id_value) => user_id_value,
        Err(status) => return koreader_auth_failure(status, header_user_presented),
    };

    let Ok(payload) = serde_json::from_slice::<KoreaderProgressPayload>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let target = match load_koreader_book_target(&app, payload.document.as_str()).await {
        Ok(Some(target)) => target,
        Ok(None) => {
            return koreader_progress_error_response(
                StatusCode::NOT_FOUND,
                "Book not found",
                KOREADER_PROGRESS_PATH,
            );
        }
        Err(KoreaderBookLookupError::Conflict) => {
            return koreader_progress_error_response(
                StatusCode::CONFLICT,
                "More than 1 book found with the same hash",
                KOREADER_PROGRESS_PATH,
            )
            .into_response();
        }
        Err(KoreaderBookLookupError::Persistence) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let (progression, use_locator_position_for_page, locator) =
        match koreader_media_profile(target.media_type.as_str()) {
            Some(KoreaderMediaProfile::Epub) => {
                match app
                    .services
                    .media_assets
                    .load_persisted_epub_extension_blob(
                        app.operational.runtime.database_file.clone(),
                        target.id.clone(),
                    )
                    .await
                {
                    Ok(Some((_extension_class, blob))) => {
                        let Ok(positions) = app.services.media_assets.decode_epub_positions(blob)
                        else {
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        };
                        let unique_hrefs = dedup_epub_hrefs(&positions);
                        let Some(resource_index) =
                            parse_koreader_epub_resource_index(payload.progress.as_str())
                        else {
                            return koreader_progress_error_response(
                                StatusCode::BAD_REQUEST,
                                &format!(
                                    "Could not get Epub resource index from progress: {}",
                                    payload.progress
                                ),
                                KOREADER_PROGRESS_PATH,
                            );
                        };
                        let Some(href) = unique_hrefs.get(resource_index) else {
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        };
                        let Some(matched_position) = positions.into_iter().find(|position| {
                            position.get("href").and_then(Value::as_str) == Some(href.as_str())
                        }) else {
                            return koreader_progress_error_response(
                                StatusCode::BAD_REQUEST,
                                &format!(
                                    "Could not get Epub resource index from progress: {}",
                                    payload.progress
                                ),
                                KOREADER_PROGRESS_PATH,
                            );
                        };
                        (
                            0.0,
                            false,
                            koreader_epub_locator(href.as_str(), &matched_position),
                        )
                    }
                    Ok(None) => {
                        return koreader_progress_error_response(
                            StatusCode::BAD_REQUEST,
                            "Epub extension not found",
                            KOREADER_PROGRESS_PATH,
                        );
                    }
                    Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            }
            Some(KoreaderMediaProfile::Visual) => {
                let Some(page) = parse_koreader_progress_page(
                    payload.progress.as_str(),
                    target.page_count,
                    payload.percentage,
                )
                .map(|value| value as i64) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                if !(1..=target.page_count.max(1) as i64).contains(&page) {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
                (
                    page as f64 / target.page_count.max(1) as f64,
                    true,
                    json!({
                        "koreaderProgress": payload.progress,
                        "locations": {
                            "position": page,
                            "totalProgression": page as f64 / target.page_count.max(1) as f64,
                        },
                    }),
                )
            }
            None => {
                return koreader_progress_error_response(
                    StatusCode::NOT_FOUND,
                    "Book has no media profile",
                    KOREADER_PROGRESS_PATH,
                );
            }
        };

    if app
        .services
        .media_assets
        .persist_book_progression(
            app.operational.runtime.database_file.clone(),
            target.id.clone(),
            user_id_value.clone(),
            progression,
            use_locator_position_for_page,
            Some(now_sync_marker()),
            Some(payload.device_id.clone()),
            Some(payload.device.clone()),
            Some(locator),
        )
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::OK.into_response()
}
