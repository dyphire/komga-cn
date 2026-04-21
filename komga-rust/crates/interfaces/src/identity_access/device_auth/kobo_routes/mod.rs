use super::*;
use crate::media_assets::access_control::user_can_access_book_media;
use crate::media_assets::http_helpers::attachment_disposition;
use crate::media_assets::read_progress::{
    normalize_book_epub_locator, progression_is_older_than_existing,
};
use axum::extract::State;
use std::sync::Arc;
mod common;
mod metadata_helpers;
mod proxy;

use common::*;
use metadata_helpers::*;
use proxy::proxied_missing_kobo_book_response;

fn encode_kobo_thumbnail_as_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Jpeg).ok()?;
    Some(output.into_inner())
}

async fn load_thumbnail_by_id(
    app: &HttpAppState,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    app.services
        .runtime_identity
        .load_thumbnail_by_id(
            app.operational.runtime.database_file.clone(),
            thumbnail_id.to_string(),
        )
        .await
}

async fn load_kobo_metadata_record(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<crate::state::KoboMetadataRecord>, sqlx::Error> {
    app.services
        .runtime_identity
        .load_kobo_metadata_record(
            app.operational.runtime.database_file.clone(),
            book_id.to_string(),
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

async fn persisted_book_exists(app: &HttpAppState, book_id: &str) -> Result<bool, sqlx::Error> {
    app.services
        .runtime_identity
        .persisted_book_exists(
            app.operational.runtime.database_file.clone(),
            book_id.to_string(),
        )
        .await
}

async fn load_book_created_timestamp(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    app.services
        .runtime_identity
        .load_book_created_timestamp(
            app.operational.runtime.database_file.clone(),
            book_id.to_string(),
        )
        .await
}

async fn load_book_last_epub_position_locator(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    app.services
        .runtime_identity
        .load_book_last_epub_position_locator(
            app.operational.runtime.database_file.clone(),
            book_id.to_string(),
        )
        .await
}

async fn load_kobo_sync_page(
    app: &HttpAppState,
    current_user: &AuthUser,
    user_id: &str,
    current_api_key_id: Option<&str>,
    ongoing_sync_point_id: Option<&str>,
    last_successful_sync_point_id: Option<&str>,
    limit: usize,
) -> Result<komga_application::identity_access::KoboSyncPage, sqlx::Error> {
    app.services
        .runtime_identity
        .load_kobo_sync_page(
            app.operational.runtime.database_file.clone(),
            current_user.clone(),
            user_id.to_string(),
            current_api_key_id.map(str::to_string),
            ongoing_sync_point_id.map(str::to_string),
            last_successful_sync_point_id.map(str::to_string),
            limit,
        )
        .await
}

async fn proxy_kobo_store_library_sync(
    app: &HttpAppState,
    headers: &HeaderMap,
    query: Option<&str>,
    raw_store_sync_token: &str,
) -> Result<komga_application::identity_access::KoboStoreSyncMergeResult, ()> {
    app.services
        .runtime_identity
        .proxy_kobo_store_library_sync(
            headers
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|value| (name.as_str().to_string(), value.to_string()))
                })
                .collect::<Vec<_>>(),
            query.map(str::to_string),
            raw_store_sync_token.to_string(),
        )
        .await
}

async fn remove_sync_point(app: &HttpAppState, sync_point_id: &str) -> Result<(), sqlx::Error> {
    app.services
        .runtime_identity
        .remove_sync_point(
            app.operational.runtime.database_file.clone(),
            sync_point_id.to_string(),
        )
        .await
}

#[allow(clippy::too_many_arguments)]
async fn kobo_book_thumbnail_response(
    app: &HttpAppState,
    server_settings: &dyn crate::state::ServerSettingsService,
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    thumbnail_id: &str,
    width: &str,
    height: &str,
) -> Response {
    if let Err(status) = required_kobo_user(
        auth_token,
        headers,
        remote_addr,
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        return status.into_response();
    }

    match load_thumbnail_by_id(app, thumbnail_id).await {
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
            if load_kobo_proxy_enabled(server_settings).await {
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

pub(super) async fn proxy_kobo_catch_all_request(
    method: &axum::http::Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, StatusCode> {
    proxy::execute_kobo_proxy_request(method, path, query, headers, body).await
}

fn sync_book_snapshot_from_metadata(
    book_id: &str,
    created: &str,
    file_last_modified: &str,
    metadata: &crate::state::KoboMetadataRecord,
) -> KoboSyncBookSnapshot {
    KoboSyncBookSnapshot {
        id: book_id.to_string(),
        title: metadata.title.clone(),
        summary: metadata.summary.clone(),
        release_date: metadata.release_date.clone(),
        language: metadata.language.clone(),
        file_size: metadata.file_size,
        page_count: 1,
        created: metadata
            .created_date
            .clone()
            .unwrap_or_else(|| created.to_string()),
        last_modified: file_last_modified.to_string(),
        contributor_names: metadata.contributor_names.clone(),
        isbn: metadata.isbn.clone(),
        publisher_name: metadata.publisher_name.clone(),
        cover_image_id: metadata.cover_image_id.clone(),
        series_id: metadata.series_id.clone(),
        series_name: metadata.series_name.clone(),
        series_number: metadata.series_number.clone(),
        series_number_float: metadata.series_number_float,
        oneshot: metadata.oneshot,
    }
}

fn removed_book_snapshot(
    book_id: &str,
    created: &str,
    file_last_modified: &str,
) -> KoboSyncBookSnapshot {
    KoboSyncBookSnapshot {
        id: book_id.to_string(),
        title: book_id.to_string(),
        summary: String::new(),
        release_date: None,
        language: "en".to_string(),
        file_size: 0,
        page_count: 1,
        created: created.to_string(),
        last_modified: file_last_modified.to_string(),
        contributor_names: Vec::new(),
        isbn: None,
        publisher_name: None,
        cover_image_id: Some(book_id.to_string()),
        series_id: None,
        series_name: None,
        series_number: None,
        series_number_float: None,
        oneshot: true,
    }
}

fn progress_snapshot(record: &PersistedReadProgressRecord) -> KoboSyncReadProgressSnapshot {
    KoboSyncReadProgressSnapshot {
        page: record.page,
        completed: record.completed,
        created: record.created.clone(),
        last_modified: record.last_modified.clone(),
        locator: record.locator.clone(),
    }
}

async fn build_kobo_sync_events_page(
    app: &HttpAppState,
    page: &komga_application::identity_access::KoboSyncPage,
    user_id: &str,
    base_url: &str,
    auth_token: &str,
) -> Result<Vec<Value>, sqlx::Error> {
    let mut events = Vec::new();

    for book in &page.books_added {
        let metadata = load_kobo_metadata_record(app, &book.book_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let progress = load_read_progress(app, &book.book_id, user_id)
            .await?
            .as_ref()
            .map(progress_snapshot);
        let snapshot = sync_book_snapshot_from_metadata(
            &book.book_id,
            &book.created,
            &book.file_last_modified,
            &metadata,
        );
        events.push(build_kobo_new_entitlement(
            &snapshot,
            progress.as_ref(),
            base_url,
            auth_token,
        ));
    }

    for book in &page.books_changed {
        let metadata = load_kobo_metadata_record(app, &book.book_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let progress = load_read_progress(app, &book.book_id, user_id)
            .await?
            .as_ref()
            .map(progress_snapshot);
        let snapshot = sync_book_snapshot_from_metadata(
            &book.book_id,
            &book.created,
            &book.file_last_modified,
            &metadata,
        );
        events.push(build_kobo_new_entitlement(
            &snapshot,
            progress.as_ref(),
            base_url,
            auth_token,
        ));
        events.push(build_kobo_changed_product_metadata(
            &snapshot, base_url, auth_token,
        ));
        if let Some(progress) = progress.as_ref() {
            events.push(build_kobo_changed_reading_state(&snapshot, progress));
        }
    }

    for book in &page.books_removed {
        let snapshot =
            removed_book_snapshot(&book.book_id, &book.created, &book.file_last_modified);
        events.push(build_kobo_changed_entitlement_removed(
            &snapshot, base_url, auth_token,
        ));
    }

    for book in &page.books_read_progress_changed {
        if let Some(progress) = load_read_progress(app, &book.book_id, user_id).await? {
            let metadata = load_kobo_metadata_record(app, &book.book_id)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            let snapshot = sync_book_snapshot_from_metadata(
                &book.book_id,
                &book.created,
                &book.file_last_modified,
                &metadata,
            );
            let progress = progress_snapshot(&progress);
            events.push(build_kobo_changed_reading_state(&snapshot, &progress));
        }
    }

    for readlist in &page.readlists_added {
        events.push(build_kobo_new_tag(readlist));
    }
    for readlist in &page.readlists_changed {
        events.push(build_kobo_changed_tag(readlist));
    }
    for readlist in &page.readlists_removed {
        events.push(build_kobo_deleted_tag(readlist));
    }

    Ok(events)
}

pub async fn kobo_library_sync(
    State(app): State<Arc<HttpAppState>>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let current_user = match required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };
    let user_id_value = user_id(&current_user).to_string();
    let current_api_key_id = resolved_kobo_request_api_key_metadata(
        &current_user,
        auth_token.as_str(),
        &headers,
        app.operational.runtime.database_file.as_path(),
    )
    .await
    .map(|(id, _)| id);
    let sync_token_raw = kobo_sync_token_from_request(&headers, &uri);
    let sync_token_payload = sync_token_raw
        .as_deref()
        .and_then(parse_komga_sync_token_payload);

    let base_url = request_base_url(&headers);
    let sync_page = match load_kobo_sync_page(
        &app,
        &current_user,
        &user_id_value,
        current_api_key_id.as_deref(),
        sync_token_payload
            .as_ref()
            .and_then(|token| token.ongoing_sync_point_id.as_deref()),
        sync_token_payload
            .as_ref()
            .and_then(|token| token.last_successful_sync_point_id.as_deref()),
        KOBO_SYNC_ITEM_LIMIT,
    )
    .await
    {
        Ok(page) => page,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let response_events = match build_kobo_sync_events_page(
        &app,
        &sync_page,
        &user_id_value,
        base_url.as_str(),
        auth_token.as_str(),
    )
    .await
    {
        Ok(events) => events,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let from_sync_point_id = sync_page.from_sync_point_id.clone();
    let to_sync_point_id = sync_page.to_sync_point_id.clone();

    let kobo_store_sync_enabled =
        load_kobo_proxy_enabled(app.services.server_settings.as_ref()).await;
    let mut merged_events = response_events;
    let mut merged_should_continue = sync_page.should_continue;
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

    if !sync_page.should_continue
        && kobo_store_sync_enabled
        && let Some(raw_store_sync_token) = merged_raw_kobo_sync_token
            .as_deref()
            .filter(|value| is_kobo_store_sync_token_candidate(value))
        && let Ok(store_response) =
            proxy_kobo_store_library_sync(&app, &headers, uri.query(), raw_store_sync_token).await
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
        && let Some(from_sync_point_id) = from_sync_point_id.as_deref()
        && from_sync_point_id != to_sync_point_id
        && remove_sync_point(&app, from_sync_point_id).await.is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let sync_token_payload_sanitized = sync_token_payload.map(|mut payload| {
        payload.ongoing_sync_point_id = sync_page.should_continue.then(|| to_sync_point_id.clone());
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
    State(app): State<Arc<HttpAppState>>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    if let Err(status) = required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
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

    let base_url = format!(
        "{}{}",
        request_base_url_with_port(&headers, Some(app.operational.runtime.bind_address.port())),
        request_context_path(&headers)
    );
    let (format, convert_kepub) = if metadata.is_pre_paginated {
        ("EPUB3FL", false)
    } else {
        ("KEPUB", !metadata.is_kepub)
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
    State(app): State<Arc<HttpAppState>>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let current_user = match required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };

    if !persisted_book_exists(&app, &book_id).await.unwrap_or(false) {
        let proxy_path = format!("/v1/library/{book_id}/state");
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

        return StatusCode::NOT_FOUND.into_response();
    }

    let user_id_value = user_id(&current_user).to_string();
    let created_timestamp = load_book_created_timestamp(&app, &book_id)
        .await
        .unwrap_or(None)
        .unwrap_or_else(now_sync_marker);

    let progress = match load_read_progress(&app, &book_id, &user_id_value).await {
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
    State(app): State<Arc<HttpAppState>>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    let current_user = match required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };

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

    if !persisted_book_exists(&app, &book_id).await.unwrap_or(false) {
        let proxy_path = format!("/v1/library/{book_id}/state");
        if let Some(response) = proxied_missing_kobo_book_response(
            &app,
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
        match load_book_last_epub_position_locator(&app, &book_id).await {
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

        match normalize_book_epub_locator(&app, &book_id, &request_locator).await {
            Ok(locator) => locator,
            Err(_) => return kobo_state_update_failure(book_id.as_str()),
        }
    };

    let stale_progression = match progression_is_older_than_existing(
        &app,
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

    let (device_id, device_name) = resolved_kobo_request_api_key_metadata(
        &current_user,
        auth_token.as_str(),
        &headers,
        app.operational.runtime.database_file.as_path(),
    )
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

    let persist_result = app
        .services
        .media_assets
        .persist_book_progression(
            app.operational.runtime.database_file.clone(),
            book_id.clone(),
            user_id_value.clone(),
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
    State(app): State<Arc<HttpAppState>>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    Query(query): Query<KoboBookFileQuery>,
) -> Response {
    let current_user = match required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };

    if !user_is_admin(&current_user) && !user_has_role(&current_user, "FILE_DOWNLOAD") {
        return StatusCode::FORBIDDEN.into_response();
    }

    let media = match app
        .services
        .media_assets
        .load_persisted_book_media(
            app.operational.runtime.database_file.clone(),
            book_id.clone(),
        )
        .await
    {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if !user_can_access_book_media(&app, &book_id, &current_user, &media).await {
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
        match app
            .services
            .media_assets
            .read_media_file_bytes(media.file_path.clone())
        {
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
    State(app): State<Arc<HttpAppState>>,
    Path((auth_token, thumbnail_id, width, height, _)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    kobo_book_thumbnail_response(
        &app,
        app.services.server_settings.as_ref(),
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        thumbnail_id.as_str(),
        width.as_str(),
        height.as_str(),
    )
    .await
}

pub async fn kobo_book_thumbnail_with_quality(
    State(app): State<Arc<HttpAppState>>,
    Path((auth_token, thumbnail_id, width, height, _, _)): Path<(
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    kobo_book_thumbnail_response(
        &app,
        app.services.server_settings.as_ref(),
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        thumbnail_id.as_str(),
        width.as_str(),
        height.as_str(),
    )
    .await
}

pub async fn kobo_catch_all(
    State(app): State<Arc<HttpAppState>>,
    Path((auth_token, path)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    method: axum::http::Method,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    if let Err(status) = required_kobo_user(
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        app.operational.runtime.database_file.as_path(),
    )
    .await
    {
        return status.into_response();
    }

    if !load_kobo_proxy_enabled(app.services.server_settings.as_ref()).await {
        return Json(json!({})).into_response();
    }

    match proxy_kobo_catch_all_request(&method, &path, uri.query(), &headers, &body).await {
        Ok(response) => response,
        Err(status) => status.into_response(),
    }
}
