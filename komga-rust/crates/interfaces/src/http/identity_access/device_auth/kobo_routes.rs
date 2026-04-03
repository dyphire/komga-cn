use super::*;

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

pub async fn kobo_library_book_metadata(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    headers: HeaderMap,
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
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let base_url = request_base_url(&headers);
    let media_type = content_type_from_filename(&metadata.file_name, "application/octet-stream");
    let format = if media_type == "application/epub+zip" {
        "EPUB"
    } else {
        "EPUB3"
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
                    "Url": format!("{base_url}/kobo/{auth_token}/v1/books/{book_id}/file/epub"),
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
        return StatusCode::NOT_FOUND.into_response();
    }

    let user_id_value = user_id(&current_user).to_string();
    let page_count = load_book_page_count(database_file, &book_id)
        .await
        .unwrap_or(1)
        .max(1);
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
            page_count,
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
    if !persisted_book_exists(database_file, &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let payload = match serde_json::from_slice::<Value>(&body) {
        Ok(payload) => payload,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid Kobo state payload" })),
            )
                .into_response();
        }
    };

    let Some(state) = payload
        .get("ReadingStates")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "ReadingStates must contain one element" })),
        )
            .into_response();
    };
    let Some(_entitlement_id) = state
        .get("EntitlementId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "EntitlementId is required" })),
        )
            .into_response();
    };

    let Some(current_bookmark) = state.get("CurrentBookmark") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "CurrentBookmark is required" })),
        )
            .into_response();
    };
    let Some(content_source_progress_percent) = current_bookmark
        .get("ContentSourceProgressPercent")
        .and_then(Value::as_f64)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "ContentSourceProgressPercent is required" })),
        )
            .into_response();
    };
    let Some(_bookmark_last_modified) = current_bookmark
        .get("LastModified")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "CurrentBookmark.LastModified is required" })),
        )
            .into_response();
    };
    let Some(_statistics_last_modified) = state
        .get("Statistics")
        .and_then(|value| value.get("LastModified"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Statistics.LastModified is required" })),
        )
            .into_response();
    };
    let Some(_status_info_last_modified) = state
        .get("StatusInfo")
        .and_then(|value| value.get("LastModified"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "StatusInfo.LastModified is required" })),
        )
            .into_response();
    };
    let Some(last_modified) = state
        .get("LastModified")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "LastModified is required" })),
        )
            .into_response();
    };
    let Some(href_source) = current_bookmark
        .get("Location")
        .and_then(|value| value.get("Source"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Location.Source is required" })),
        )
            .into_response();
    };

    let content_source_progress = content_source_progress_percent / 100.0;
    let total_progress = current_bookmark
        .get("ProgressPercent")
        .and_then(Value::as_f64)
        .unwrap_or(content_source_progress * 100.0)
        / 100.0;
    let total_progress = total_progress.clamp(0.0, 1.0);
    let content_source_progress = content_source_progress.clamp(0.0, 1.0);
    let Some(status) = state
        .get("StatusInfo")
        .and_then(|value| value.get("Status"))
        .and_then(Value::as_str)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "StatusInfo.Status is required" })),
        )
            .into_response();
    };
    let completed = status.eq_ignore_ascii_case("Finished");
    let progress_last_modified = last_modified.to_string();
    let page_count = load_book_page_count(database_file, &book_id)
        .await
        .unwrap_or(1)
        .max(1);
    let computed_page = if completed {
        page_count
    } else {
        ((total_progress * page_count as f64).ceil() as u64).clamp(0, page_count)
    };
    let page = computed_page.max(1) as i64;

    let href = href_source.to_string();
    let kobo_span = current_bookmark
        .get("Location")
        .and_then(|value| value.get("Value"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let user_id_value = user_id(&current_user).to_string();
    let request_locator = json!({
        "href": href,
        "type": "application/xhtml+xml",
        "koboSpan": if kobo_span.is_empty() { Value::Null } else { Value::String(kobo_span) },
        "locations": {
            "progression": content_source_progress,
            "totalProgression": total_progress,
            "position": page,
        },
    });

    let locator = if completed {
        match load_book_last_epub_position_locator(database_file, &book_id).await {
            Ok(Some(locator)) => locator,
            _ => {
                return kobo_state_update_failure(book_id.as_str());
            }
        }
    } else {
        request_locator
    };

    let (device_id, device_name) = configured_api_key_identity(
        auth_token.as_str(),
        configured_api_key().as_deref(),
        configured_api_key_id().as_deref(),
        configured_api_key_comment().as_deref(),
    );

    let persist_result = persist_read_progress_with_locator(
        database_file,
        &book_id,
        &user_id_value,
        page,
        completed,
        &device_id,
        &device_name,
        progress_last_modified.as_str(),
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

    let media = match load_book_media_file(state.runtime.database_file.as_path(), &book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

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
        match std::fs::read(&media.file_path) {
            Ok(body) => body,
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        }
    };

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
                HeaderValue::from_str(format!("attachment; filename=\"{}\"", file_name).as_str())
                    .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
            ),
        ],
        body,
    )
        .into_response()
}

pub async fn kobo_book_thumbnail(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, thumbnail_id, _, _, _)): Path<(String, String, String, String, String)>,
    headers: HeaderMap,
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

    match load_thumbnail_by_id(state.runtime.database_file.as_path(), &thumbnail_id).await {
        Ok(Some((media_type, bytes))) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
            )],
            bytes,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn kobo_book_thumbnail_with_quality(
    Extension(state): Extension<OperationalState>,
    Path((auth_token, thumbnail_id, _, _, _, _)): Path<(
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    headers: HeaderMap,
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

    match load_thumbnail_by_id(state.runtime.database_file.as_path(), &thumbnail_id).await {
        Ok(Some((media_type, bytes))) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
            )],
            bytes,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
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

async fn proxy_kobo_catch_all_request(
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
