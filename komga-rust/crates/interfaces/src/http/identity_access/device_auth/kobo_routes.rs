use super::*;
use crate::http::media_assets::{
    attachment_disposition, normalize_book_epub_locator, progression_is_older_than_existing,
    user_can_access_book_media,
};
use crate::runtime_identity_access::persisted_api_key_metadata;

fn encode_kobo_thumbnail_as_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Jpeg).ok()?;
    Some(output.into_inner())
}

async fn kobo_book_thumbnail_response(
    state: &OperationalState,
    auth_token: &str,
    headers: &HeaderMap,
    thumbnail_id: &str,
    width: &str,
    height: &str,
) -> Response {
    if resolved_kobo_user(auth_token, headers, state.runtime.database_file.as_path())
        .await
        .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    match load_thumbnail_by_id(state.runtime.database_file.as_path(), thumbnail_id).await {
        Ok(Some((media_type, bytes))) => {
            let jpeg_bytes = if media_type.eq_ignore_ascii_case("image/jpeg") {
                bytes
            } else {
                match encode_kobo_thumbnail_as_jpeg(&bytes) {
                    Some(jpeg_bytes) => jpeg_bytes,
                    None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            };

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"))],
                jpeg_bytes,
            )
                .into_response()
        }
        Ok(None) => {
            if load_kobo_proxy_enabled(state).await {
                let location = format!(
                    "https://cdn.kobo.com/book-images/{thumbnail_id}/{width}/{height}/false/image.jpg"
                );
                return (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(
                        header::LOCATION,
                        HeaderValue::from_str(location.as_str()).unwrap_or_else(|_| {
                            HeaderValue::from_static(
                                "https://cdn.kobo.com/book-images/invalid/0/0/false/image.jpg",
                            )
                        }),
                    )],
                )
                    .into_response();
            }
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn kobo_description(summary: &str) -> Value {
    if summary.trim().is_empty() {
        Value::String(" ".to_string())
    } else {
        Value::String(summary.to_string())
    }
}

fn kobo_language(language: &str) -> String {
    let language = language.trim();
    if language.is_empty() {
        "en".to_string()
    } else {
        language
            .chars()
            .take(2)
            .collect::<String>()
            .to_ascii_lowercase()
    }
}

fn kobo_publication_date_value(value: &str) -> Option<Value> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else if value.len() == 10 && value.as_bytes().get(4) == Some(&b'-') {
        Some(Value::String(format!("{value}T00:00:00Z")))
    } else {
        Some(Value::String(value.to_string()))
    }
}

pub async fn kobo_library_sync(
    Extension(state): Extension<OperationalState>,
    Path(auth_token): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let Some(current_user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id_value = user_id(&current_user).to_string();
    let current_api_key_id =
        kobo_path_api_key_id(auth_token.as_str(), state.runtime.database_file.as_path()).await;
    let sync_token_raw = kobo_sync_token_from_request(&headers, &uri);
    let sync_token_payload = sync_token_raw
        .as_deref()
        .and_then(parse_komga_sync_token_payload);

    let ongoing_sync_point_id = if let Some(id) = sync_token_payload
        .as_ref()
        .and_then(|token| token.ongoing_sync_point_id.clone())
    {
        if load_sync_point_state(state.runtime.database_file.as_path(), &id, &user_id_value)
            .await
            .is_some()
        {
            Some(id)
        } else {
            None
        }
    } else {
        None
    };
    let last_successful_sync_point_id = if let Some(id) = sync_token_payload
        .as_ref()
        .and_then(|token| token.last_successful_sync_point_id.clone())
    {
        if load_sync_point_state(state.runtime.database_file.as_path(), &id, &user_id_value)
            .await
            .is_some()
        {
            Some(id)
        } else {
            None
        }
    } else {
        None
    };
    let from_sync_point_id = last_successful_sync_point_id.clone();

    let to_sync_point_id = ongoing_sync_point_id
        .clone()
        .unwrap_or_else(random_uuid_like);
    let mut to_sync_point_state = if let Some(state_entry) = load_sync_point_state(
        state.runtime.database_file.as_path(),
        to_sync_point_id.as_str(),
        &user_id_value,
    )
    .await
    {
        state_entry
    } else {
        KoboSyncPointState {
            user_id: user_id_value.clone(),
            api_key_id: current_api_key_id.clone(),
            marker: now_sync_marker(),
            cursor: 0,
            from_marker: if let Some(sync_id) = from_sync_point_id.as_ref() {
                load_sync_point_marker(
                    state.runtime.database_file.as_path(),
                    sync_id,
                    &user_id_value,
                )
                .await
            } else {
                None
            }
            .or(sync_token_raw.clone()),
            snapshot: None,
        }
    };

    if to_sync_point_state.from_marker.is_none() {
        let marker = if let Some(sync_id) = from_sync_point_id.as_ref() {
            load_sync_point_marker(
                state.runtime.database_file.as_path(),
                sync_id,
                &user_id_value,
            )
            .await
        } else {
            None
        };
        to_sync_point_state.from_marker = marker.or(sync_token_raw.clone());
    }

    if to_sync_point_state.snapshot.is_none() {
        to_sync_point_state.snapshot =
            match load_kobo_sync_snapshot(state.runtime.database_file.as_path(), &user_id_value)
                .await
            {
                Ok(snapshot) => Some(snapshot),
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
    }

    let from_sync_snapshot = if let Some(sync_id) = from_sync_point_id.as_ref() {
        load_sync_point_state(
            state.runtime.database_file.as_path(),
            sync_id,
            &user_id_value,
        )
        .await
        .and_then(|sync_state| sync_state.snapshot)
    } else {
        None
    };

    let base_url = request_base_url(&headers);
    let events = build_kobo_sync_events(
        from_sync_snapshot.as_ref(),
        to_sync_point_state
            .snapshot
            .as_ref()
            .expect("snapshot initialized"),
        base_url.as_str(),
        auth_token.as_str(),
    );

    let start_index = to_sync_point_state.cursor.min(events.len());
    let end_index = (start_index + KOBO_SYNC_ITEM_LIMIT).min(events.len());
    let response_events = events[start_index..end_index].to_vec();
    let should_continue = end_index < events.len();

    to_sync_point_state.cursor = if should_continue { end_index } else { 0 };
    if save_sync_point(
        state.runtime.database_file.as_path(),
        to_sync_point_id.as_str(),
        &to_sync_point_state,
    )
    .await
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let kobo_store_sync_enabled = load_kobo_proxy_enabled(&state).await;
    let mut merged_events = response_events;
    let mut merged_should_continue = should_continue;
    let mut merged_raw_kobo_sync_token = sync_token_payload
        .as_ref()
        .map(|payload| payload.raw_kobo_sync_token.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            sync_token_raw
                .as_deref()
                .filter(|value| is_kobo_store_sync_token_candidate(value))
                .map(str::to_string)
        });

    if !should_continue
        && kobo_store_sync_enabled
        && let Some(raw_store_sync_token) = merged_raw_kobo_sync_token
            .as_deref()
            .filter(|value| is_kobo_store_sync_token_candidate(value))
        && let Ok(store_response) = proxy_kobo_store_library_sync(
            &headers
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|value| (name.as_str().to_string(), value.to_string()))
                })
                .collect::<Vec<_>>(),
            uri.query(),
            raw_store_sync_token,
        )
        .await
    {
        merged_events.extend(store_response.events);
        merged_should_continue = store_response.should_continue;
        if let Some(raw_store_sync_token) = store_response.raw_sync_token
            && !raw_store_sync_token.trim().is_empty()
        {
            merged_raw_kobo_sync_token = Some(raw_store_sync_token);
        }
    }

    if !merged_should_continue
        && let Some(from_sync_point_id) = from_sync_point_id
        && from_sync_point_id != to_sync_point_id
        && remove_sync_point(
            state.runtime.database_file.as_path(),
            from_sync_point_id.as_str(),
        )
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let sync_token_payload_sanitized = sync_token_payload.map(|mut payload| {
        payload.ongoing_sync_point_id = ongoing_sync_point_id;
        if let Some(raw) = merged_raw_kobo_sync_token.as_ref() {
            payload.raw_kobo_sync_token = raw.clone();
        }
        payload
    });
    let sync_token_response = build_komga_sync_token_payload(
        sync_token_payload_sanitized,
        merged_raw_kobo_sync_token,
        to_sync_point_id.as_str(),
        merged_should_continue,
    );
    let encoded_sync_token = format!("KOMGA.{}", STANDARD_NO_PAD.encode(sync_token_response));

    let mut response = (
        StatusCode::OK,
        [(
            HeaderName::from_static("x-kobo-synctoken"),
            HeaderValue::from_str(encoded_sync_token.as_str())
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        )],
        Json(Value::Array(merged_events)),
    )
        .into_response();
    if merged_should_continue {
        response.headers_mut().insert(
            HeaderName::from_static("x-kobo-sync"),
            HeaderValue::from_static("continue"),
        );
    }
    response
}

async fn kobo_path_api_key_id(auth_token: &str, database_file: &FsPath) -> Option<String> {
    kobo_path_api_key_metadata(auth_token, database_file)
        .await
        .map(|(id, _)| id)
}

async fn kobo_path_api_key_metadata(
    auth_token: &str,
    database_file: &FsPath,
) -> Option<(String, String)> {
    if !valid_kobo_path_token(auth_token) {
        return None;
    }

    let mut metadata_headers = HeaderMap::new();
    metadata_headers.insert("x-api-key", HeaderValue::from_str(auth_token).ok()?);
    persisted_api_key_metadata(&metadata_headers, database_file)
        .await
        .map(|metadata| (metadata.id, metadata.comment))
}

async fn proxied_missing_kobo_book_response(
    state: &OperationalState,
    method: &axum::http::Method,
    proxy_path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Option<Response> {
    if !load_kobo_proxy_enabled(state).await {
        return None;
    }

    Some(
        match proxy_kobo_catch_all_request(method, proxy_path, query, headers, body).await {
            Ok(response) => response,
            Err(status) => status.into_response(),
        },
    )
}

async fn built_in_kepub_conversion_available(state: &OperationalState) -> bool {
    let persisted = state.settings_store.load_map().await.ok();
    let configured_path = persisted
        .as_ref()
        .and_then(|settings| settings.get("KEPUBIFY_PATH"))
        .and_then(|value| value.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let fallback_path = state.runtime.config_dir.as_ref().and_then(|config_dir| {
        config_kepubify_path(config_dir.join("application.yml").as_path())
            .or_else(|| config_kepubify_path(config_dir.join("application.yaml").as_path()))
            .or_else(|| {
                properties_kepubify_path(config_dir.join("application.properties").as_path())
            })
    });

    if let Some(path) = configured_path {
        if kepubify_path_is_available(path.as_str()) {
            return true;
        }
        return fallback_path
            .as_deref()
            .is_some_and(kepubify_path_is_available);
    }

    fallback_path
        .as_deref()
        .is_some_and(kepubify_path_is_available)
}

fn config_kepubify_path(path: &FsPath) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|content| {
        content.lines().find_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("kepubify-path:") {
                return None;
            }
            trimmed
                .split_once(':')
                .map(|(_, value)| {
                    value
                        .trim()
                        .trim_matches(|character| character == '"' || character == '\'')
                        .to_string()
                })
                .filter(|value| !value.is_empty())
        })
    })
}

fn properties_kepubify_path(path: &FsPath) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|content| {
        content.lines().find_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("komga.kobo.kepubify-path=") {
                return None;
            }
            trimmed
                .split_once('=')
                .map(|(_, value)| {
                    value
                        .trim()
                        .trim_matches(|character| character == '"' || character == '\'')
                        .to_string()
                })
                .filter(|value| !value.is_empty())
        })
    })
}

fn kepubify_path_is_available(path: &str) -> bool {
    let candidate = std::path::Path::new(path);
    if (candidate.is_absolute() || path.contains(std::path::MAIN_SEPARATOR))
        && let Ok(metadata) = std::fs::metadata(candidate)
    {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            return metadata.is_file() && metadata.permissions().mode() & 0o111 != 0;
        }
        #[cfg(not(unix))]
        {
            return metadata.is_file();
        }
    }

    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths)
        .map(|directory| directory.join(path))
        .any(|candidate| {
            std::fs::metadata(&candidate).ok().is_some_and(|metadata| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    metadata.is_file()
                }
            })
        })
}

pub async fn kobo_library_book_metadata(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let database_file = state.runtime.database_file.as_path();
    let metadata = match load_kobo_metadata_record(database_file, &book_id).await {
        Ok(Some(metadata)) => metadata,
        Ok(None) => {
            let book_exists = persisted_book_exists(database_file, &book_id)
                .await
                .unwrap_or(false);
            if !book_exists {
                let proxy_path = format!("/v1/library/{book_id}/metadata");
                if let Some(response) = proxied_missing_kobo_book_response(
                    &state,
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

    let base_url = format!(
        "{}{}",
        request_base_url_with_port(&headers, Some(state.runtime.bind_address.port())),
        request_context_path(&headers)
    );
    let (format, convert_kepub) = if metadata.is_pre_paginated {
        ("EPUB3FL", false)
    } else if metadata.is_kepub || built_in_kepub_conversion_available(&state).await {
        ("KEPUB", !metadata.is_kepub)
    } else {
        ("EPUB3", false)
    };
    let contributor_roles = metadata
        .contributor_names
        .iter()
        .map(|name| json!({ "Name": name }))
        .collect::<Vec<_>>();
    let contributors = metadata
        .contributor_names
        .iter()
        .map(|name| Value::String(name.clone()))
        .collect::<Vec<_>>();
    let publication_date = metadata
        .release_date
        .as_deref()
        .or(metadata.created_date.as_deref())
        .and_then(kobo_publication_date_value)
        .unwrap_or(Value::Null);
    let publisher = metadata
        .publisher_name
        .as_ref()
        .map(|name| json!({ "Imprint": "", "Name": name }))
        .unwrap_or(Value::Null);
    let series = if metadata.oneshot {
        Value::Null
    } else if let (
        Some(series_id),
        Some(series_name),
        Some(series_number),
        Some(series_number_float),
    ) = (
        metadata.series_id.as_ref(),
        metadata.series_name.as_ref(),
        metadata.series_number.as_ref(),
        metadata.series_number_float,
    ) {
        json!({
            "Id": series_id,
            "Name": series_name,
            "Number": series_number,
            "NumberFloat": series_number_float,
        })
    } else {
        Value::Null
    };

    Json(json!([
        {
            "Categories": ["00000000-0000-0000-0000-000000000001"],
            "ContributorRoles": contributor_roles,
            "Contributors": contributors,
            "CoverImageId": metadata.cover_image_id,
            "CrossRevisionId": book_id,
            "CurrentDisplayPrice": {"CurrencyCode": "USD", "TotalAmount": 0},
            "CurrentLoveDisplayPrice": {"CurrencyCode": "USD", "TotalAmount": 0},
            "Description": kobo_description(&metadata.summary),
            "DownloadUrls": [
                {
                    "DrmType": "None",
                    "Format": format,
                    "Platform": "Generic",
                    "Size": metadata.file_size,
                    "Url": format!("{base_url}/kobo/{auth_token}/v1/books/{book_id}/file/epub?convert_kepub={convert_kepub}"),
                }
            ],
            "EntitlementId": book_id,
            "ExternalIds": [],
            "Genre": "00000000-0000-0000-0000-000000000001",
            "IsEligibleForKoboLove": false,
            "IsInternetArchive": false,
            "IsPreOrder": false,
            "IsSocialEnabled": true,
            "ISBN": metadata.isbn,
            "Language": kobo_language(&metadata.language),
            "PhoneticPronunciations": {},
            "PublicationDate": publication_date,
            "Publisher": publisher,
            "RevisionId": book_id,
            "Series": series,
            "Slug": Value::Null,
            "SubTitle": Value::Null,
            "Title": metadata.title,
            "WorkId": book_id,
        }
    ]))
    .into_response()
}

pub async fn kobo_library_book_state(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let Some(current_user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let database_file = state.runtime.database_file.as_path();
    if !persisted_book_exists(database_file, &book_id)
        .await
        .unwrap_or(false)
    {
        let proxy_path = format!("/v1/library/{book_id}/state");
        if let Some(response) = proxied_missing_kobo_book_response(
            &state,
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

        return StatusCode::NOT_FOUND.into_response();
    }

    let user_id_value = user_id(&current_user).to_string();
    let created_timestamp = load_book_created_timestamp(database_file, &book_id)
        .await
        .unwrap_or(None)
        .unwrap_or_else(now_sync_marker);

    let progress = match load_read_progress(database_file, &book_id, &user_id_value).await {
        Ok(progress) => progress,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let payload = match progress {
        Some(record) => kobo_reading_state_payload(
            &book_id,
            &record,
            parse_locator_payload(record.locator.as_deref()),
        ),
        None => kobo_empty_reading_state_payload(&book_id, created_timestamp.as_str()),
    };

    Json(json!([payload])).into_response()
}

pub async fn kobo_library_book_state_update(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    let Some(current_user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let database_file = state.runtime.database_file.as_path();
    let payload = match serde_json::from_slice::<KoboReadingStateUpdatePayload>(&body) {
        Ok(payload) => payload,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid Kobo state payload" })),
            )
                .into_response();
        }
    };

    if !persisted_book_exists(database_file, &book_id)
        .await
        .unwrap_or(false)
    {
        let proxy_path = format!("/v1/library/{book_id}/state");
        if let Some(response) = proxied_missing_kobo_book_response(
            &state,
            &axum::http::Method::PUT,
            proxy_path.as_str(),
            uri.query(),
            &headers,
            &body,
        )
        .await
        {
            return response;
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(state) = payload.reading_states.first() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "ReadingStates must contain one element" })),
        )
            .into_response();
    };
    let Some(location) = state.current_bookmark.location.as_ref() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(content_source_progress_percent) =
        state.current_bookmark.content_source_progress_percent
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let content_source_progress = content_source_progress_percent / 100.0;
    let total_progress = state
        .current_bookmark
        .progress_percent
        .map(|value| value / 100.0);
    let completed = state.status_info.status.eq_ignore_ascii_case("Finished");
    let progress_last_modified = state.last_modified.clone();
    let kobo_span = if location.location_type.eq_ignore_ascii_case("kobospan") {
        location.value.clone()
    } else {
        None
    };
    let user_id_value = user_id(&current_user).to_string();

    let locator = if completed {
        match load_book_last_epub_position_locator(database_file, &book_id).await {
            Ok(Some(locator)) => locator,
            _ => {
                return kobo_state_update_failure(book_id.as_str());
            }
        }
    } else {
        let request_locator = json!({
            "href": location.source,
            "type": "application/xhtml+xml",
            "koboSpan": kobo_span,
            "locations": {
                "progression": content_source_progress,
                "totalProgression": total_progress,
            },
        });

        match normalize_book_epub_locator(database_file, &book_id, &request_locator).await {
            Ok(locator) => locator,
            Err(_) => return kobo_state_update_failure(book_id.as_str()),
        }
    };

    let stale_progression = match progression_is_older_than_existing(
        database_file,
        &book_id,
        &user_id_value,
        progress_last_modified.as_str(),
    )
    .await
    {
        Ok(stale) => stale,
        Err(_) => return kobo_state_update_failure(book_id.as_str()),
    };
    if stale_progression {
        return kobo_state_update_failure(book_id.as_str());
    }

    let (device_id, device_name) = kobo_path_api_key_metadata(auth_token.as_str(), database_file)
        .await
        .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

    let locator_progression = locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
        .unwrap_or(if completed {
            1.0
        } else {
            content_source_progress
        });

    let persist_result = persist_book_progression(
        database_file,
        &book_id,
        &user_id_value,
        locator_progression,
        false,
        Some(progress_last_modified),
        Some(device_id),
        Some(device_name),
        Some(locator),
    )
    .await;

    let update_result = if persist_result.is_ok() {
        "Success"
    } else {
        "Failure"
    };

    Json(json!({
        "RequestResult": update_result,
        "UpdateResults": [
            {
                "EntitlementId": book_id,
                "CurrentBookmarkResult": {"Result": update_result},
                "StatisticsResult": {"Result": if persist_result.is_ok() { "Ignored" } else { "Failure" }},
                "StatusInfoResult": {"Result": update_result},
            }
        ],
    }))
    .into_response()
}

fn kobo_state_update_failure(book_id: &str) -> Response {
    Json(json!({
        "RequestResult": "Failure",
        "UpdateResults": [
            {
                "EntitlementId": book_id,
                "CurrentBookmarkResult": {"Result": "Failure"},
                "StatisticsResult": {"Result": "Failure"},
                "StatusInfoResult": {"Result": "Failure"},
            }
        ],
    }))
    .into_response()
}

pub async fn kobo_book_file_epub(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<KoboBookFileQuery>,
) -> Response {
    let Some(current_user) = resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    if !user_is_admin(&current_user) && !user_has_role(&current_user, "FILE_DOWNLOAD") {
        return StatusCode::FORBIDDEN.into_response();
    }

    let media =
        match load_persisted_book_media(state.runtime.database_file.as_path(), &book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

    if !user_can_access_book_media(
        state.runtime.database_file.as_path(),
        &book_id,
        &current_user,
        &media,
    )
    .await
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let mut file_name = media.file_name.clone();
    let mut media_type = media.media_type.clone();

    let body = if query.convert_kepub.unwrap_or(false) && media.media_type == "application/epub+zip"
    {
        if let Some(converted_body) = convert_epub_to_kepub_bytes(&media.file_path) {
            file_name = kobo_kepub_file_name(media.file_name.as_str());
            media_type = "application/epub+zip".to_string();
            converted_body
        } else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "Kepub conversion failed" })),
            )
                .into_response();
        }
    } else {
        match read_media_file_bytes(&media.file_path) {
            Some(body) => body,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "File not found, it may have moved" })),
                )
                    .into_response();
            }
        }
    };

    let content_disposition = attachment_disposition(&file_name);

    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            ),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(content_disposition.as_str())
                    .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
            ),
        ],
        body,
    )
        .into_response()
}

pub async fn kobo_book_thumbnail(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, thumbnail_id, width, height, _)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
    headers: HeaderMap,
) -> Response {
    kobo_book_thumbnail_response(
        &state,
        auth_token.as_str(),
        &headers,
        thumbnail_id.as_str(),
        width.as_str(),
        height.as_str(),
    )
    .await
}

pub async fn kobo_book_thumbnail_with_quality(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, thumbnail_id, width, height, _, _)): Path<(
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    headers: HeaderMap,
) -> Response {
    kobo_book_thumbnail_response(
        &state,
        auth_token.as_str(),
        &headers,
        thumbnail_id.as_str(),
        width.as_str(),
        height.as_str(),
    )
    .await
}

pub async fn kobo_catch_all(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, path)): Path<(String, String)>,
    headers: HeaderMap,
    method: axum::http::Method,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    if resolved_kobo_user(
        auth_token.as_str(),
        &headers,
        state.runtime.database_file.as_path(),
    )
    .await
    .is_none()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if !load_kobo_proxy_enabled(&state).await {
        return Json(json!({})).into_response();
    }

    match proxy_kobo_catch_all_request(&method, &path, uri.query(), &headers, &body).await {
        Ok(response) => response,
        Err(status) => status.into_response(),
    }
}

pub(super) async fn proxy_kobo_catch_all_request(
    method: &axum::http::Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, StatusCode> {
    let base_url = std::env::var("KOMGA_RUST_KOBO_PROXY_URL")
        .unwrap_or_else(|_| "https://storeapi.kobo.com".to_string());
    let mut target = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }

    let client = Client::builder()
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let request_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut request = client.request(request_method, target);

    for (name, value) in headers {
        let header_name = name.as_str();
        let lower = header_name.to_ascii_lowercase();
        let should_forward = matches!(
            lower.as_str(),
            "authorization" | "user-agent" | "accept" | "accept-language" | "content-type"
        ) || lower.starts_with("x-kobo-");
        if !should_forward || lower == "x-kobo-synctoken" {
            continue;
        }
        let Ok(value) = value.to_str() else {
            continue;
        };
        request = request.header(header_name, value);
    }

    if let Some(prepared_body) = prepare_kobo_proxy_request_body(headers, body)? {
        request = request.body(prepared_body);
    }

    let response = request
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let response_headers = response.headers().clone();
    let response_bytes = response
        .bytes()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if status.is_client_error() || status.is_server_error() {
        return Ok(status.into_response());
    }
    if response_bytes.is_empty() {
        let mut proxied = status.into_response();
        for (name, value) in &response_headers {
            if name.as_str().to_ascii_lowercase().starts_with("x-kobo-") {
                proxied.headers_mut().append(name.clone(), value.clone());
            }
        }
        return Ok(proxied);
    }

    let (mut proxied, include_kobo_headers) = match serde_json::from_slice::<Value>(&response_bytes)
    {
        Ok(response_body) => {
            let mut response = Json(response_body).into_response();
            *response.status_mut() = status;
            (response, true)
        }
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    if include_kobo_headers {
        for (name, value) in &response_headers {
            if name.as_str().to_ascii_lowercase().starts_with("x-kobo-") {
                proxied.headers_mut().append(name.clone(), value.clone());
            }
        }
    }
    Ok(proxied)
}

fn prepare_kobo_proxy_request_body(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Option<Vec<u8>>, StatusCode> {
    if body.is_empty() {
        return Ok(None);
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase());
    let is_json = content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("application/json") || value.contains("+json"));
    let is_xml = content_type.as_deref().is_some_and(|value| {
        value.starts_with("application/xml")
            || value.starts_with("text/xml")
            || value.contains("+xml")
    });

    if is_xml {
        validate_kobo_xml_request_body(body)?;
        return Ok(Some(body.to_vec()));
    }

    if !is_json {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    let value = serde_json::from_slice::<Value>(body).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Some(value.to_string().into_bytes()))
}

fn validate_kobo_xml_request_body(body: &Bytes) -> Result<(), StatusCode> {
    let mut reader = quick_xml::Reader::from_reader(body.as_ref());
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Eof) => return Ok(()),
            Ok(_) => buffer.clear(),
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        }
    }
}

fn random_uuid_like() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hex = format!("{nanos:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    )
}
