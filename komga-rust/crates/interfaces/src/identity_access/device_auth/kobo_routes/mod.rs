use super::*;
use crate::media_assets::access_control::user_can_access_book_media;
use crate::media_assets::http_helpers::attachment_disposition;
use crate::media_assets::read_progress::{
    normalize_book_epub_locator, progression_is_older_than_existing,
};
use crate::state::IdentityAccessState;
use axum::extract::State;
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
    app: &IdentityAccessState,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, String> {
    app.identity
        .device_sync()
        .load_thumbnail_by_id(thumbnail_id)
        .await
}

async fn load_kobo_metadata_record(
    app: &IdentityAccessState,
    book_id: &str,
) -> Result<Option<crate::state::KoboMetadataRecord>, String> {
    app.identity
        .device_sync()
        .load_kobo_metadata_record(book_id)
        .await
}

async fn load_read_progress(
    app: &IdentityAccessState,
    book_id: &str,
    user_id: &str,
) -> Result<Option<PersistedReadProgressRecord>, String> {
    app.identity
        .device_sync()
        .load_read_progress(book_id, user_id)
        .await
}

async fn persisted_book_exists(app: &IdentityAccessState, book_id: &str) -> Result<bool, String> {
    app.identity
        .device_sync()
        .persisted_book_exists(book_id)
        .await
}

async fn load_book_created_timestamp(
    app: &IdentityAccessState,
    book_id: &str,
) -> Result<Option<String>, String> {
    app.identity
        .device_sync()
        .load_book_created_timestamp(book_id)
        .await
}

async fn load_book_last_epub_position_locator(
    app: &IdentityAccessState,
    book_id: &str,
) -> Result<Option<Value>, String> {
    app.identity
        .device_sync()
        .load_book_last_epub_position_locator(book_id)
        .await
}

#[allow(clippy::too_many_arguments)]
async fn kobo_book_thumbnail_response(
    app: &IdentityAccessState,
    server_settings: &dyn komga_application::operational::ServerSettingsPort,
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    thumbnail_id: &str,
    width: &str,
    height: &str,
) -> Response {
    if let Err(status) = required_kobo_user(&app.identity, auth_token, headers, remote_addr).await {
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

pub async fn kobo_library_sync(
    State(app): State<IdentityAccessState>,
    Path(auth_token): Path<String>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };
    let current_api_key_id = resolved_kobo_request_api_key_metadata(
        &app.identity,
        &current_user,
        auth_token.as_str(),
        &headers,
    )
    .await
    .map(|(id, _)| id);
    let sync_token_raw = kobo_sync_token_from_request(&headers, &uri);
    let base_url = kobo_request_base_url(&app, &headers).await;
    let forwarded_headers = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect::<Vec<_>>();
    let store_sync_enabled = load_kobo_proxy_enabled(app.server_settings.as_ref()).await;
    let sync_response = match app
        .identity
        .device_sync()
        .load_kobo_library_sync(KoboLibrarySyncRequest {
            user: current_user,
            current_api_key_id,
            sync_token_raw,
            store_sync_enabled,
            forwarded_headers,
            query: uri.query().map(str::to_string),
            base_url,
            auth_token,
            limit: KOBO_SYNC_ITEM_LIMIT,
        })
        .await
    {
        Ok(response) => response,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let encoded_sync_token = format!(
        "KOMGA.{}",
        STANDARD_NO_PAD.encode(sync_response.sync_token_payload)
    );

    let mut response = (
        StatusCode::OK,
        [(
            HeaderName::from_static("x-kobo-synctoken"),
            HeaderValue::from_str(encoded_sync_token.as_str())
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        )],
        Json(Value::Array(sync_response.events)),
    )
        .into_response();
    if sync_response.should_continue {
        response.headers_mut().insert(
            HeaderName::from_static("x-kobo-sync"),
            HeaderValue::from_static("continue"),
        );
    }
    response
}

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
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
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

    let user_id_value = user_id(&current_user);
    let created_timestamp = load_book_created_timestamp(&app, &book_id)
        .await
        .unwrap_or(None)
        .unwrap_or_else(now_sync_marker);

    let progress = match load_read_progress(&app, &book_id, user_id_value).await {
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
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
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
    let user_id_value = user_id(&current_user);

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

        match normalize_book_epub_locator(app.reader.as_ref(), &book_id, &request_locator).await {
            Ok(locator) => locator,
            Err(_) => return kobo_state_update_failure(book_id.as_str()),
        }
    };

    let stale_progression = match progression_is_older_than_existing(
        app.reader.as_ref(),
        &book_id,
        user_id_value,
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
        &app.identity,
        &current_user,
        auth_token.as_str(),
        &headers,
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
        .progress
        .persist_book_progression(
            &book_id,
            user_id_value,
            locator_progression,
            false,
            Some(progress_last_modified.clone()),
            Some(device_id.to_string()),
            Some(device_name.to_string()),
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
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    Query(query): Query<KoboBookFileQuery>,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };

    if !user_is_admin(&current_user) && !user_has_role(&current_user, "FILE_DOWNLOAD") {
        return StatusCode::FORBIDDEN.into_response();
    }

    let media = match app.reader.book_media(&book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if !user_can_access_book_media(app.reader.as_ref(), &book_id, &current_user, &media).await {
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
        match app.content.read_media_file_bytes(&media.file_path).await {
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
    State(app): State<IdentityAccessState>,
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
        app.server_settings.as_ref(),
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
    State(app): State<IdentityAccessState>,
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
        app.server_settings.as_ref(),
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
    State(app): State<IdentityAccessState>,
    Path((auth_token, path)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    method: axum::http::Method,
    uri: axum::http::Uri,
    body: Bytes,
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

    if !load_kobo_proxy_enabled(app.server_settings.as_ref()).await {
        return Json(json!({})).into_response();
    }

    match proxy_kobo_catch_all_request(&method, &path, uri.query(), &headers, &body).await {
        Ok(response) => response,
        Err(status) => status.into_response(),
    }
}
