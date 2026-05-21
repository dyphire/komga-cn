use super::*;
use crate::identity_access::auth::Authenticated;
use crate::media_assets::page_resolution;
use crate::media_responses::{self, BookPageResponseOptions};
use crate::opds::content_opds::OpdsV1Authenticated;
use crate::state::MediaAssetsState;
use axum::extract::State;
use komga_application::media_assets::BookPageRecord;

pub async fn book_page(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    media_responses::book_page_response(
        &app.reader,
        &app.content,
        app.discovery_detail.as_ref(),
        &user,
        &headers,
        &book_id,
        page_number,
        query.into_response_options(),
    )
    .await
}

pub async fn book_page_opds_v1(
    State(app): State<MediaAssetsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    Query(mut query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    query.zero_based = true;
    query.content_negotiation = false;
    media_responses::book_page_response(
        &app.reader,
        &app.content,
        app.discovery_detail.as_ref(),
        &user,
        &headers,
        &book_id,
        page_number,
        query.into_response_options(),
    )
    .await
}

pub async fn book_page_raw(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, page_number_signed)): Path<(String, i32)>,
) -> Response {
    media_responses::book_page_raw_response(
        &app.reader,
        &app.content,
        app.discovery_detail.as_ref(),
        &user,
        &headers,
        &book_id,
        page_number_signed,
    )
    .await
}

#[derive(Deserialize, Default)]
pub struct BookPageQuery {
    #[serde(default)]
    pub(crate) convert: Option<String>,

    #[serde(default)]
    pub(crate) zero_based: bool,

    #[serde(default = "book_page_content_negotiation_default")]
    #[serde(rename = "contentNegotiation")]
    pub(crate) content_negotiation: bool,
}

impl BookPageQuery {
    pub(crate) fn into_response_options(self) -> BookPageResponseOptions {
        BookPageResponseOptions {
            convert: self.convert,
            zero_based: self.zero_based,
            content_negotiation: self.content_negotiation,
        }
    }
}

fn book_page_content_negotiation_default() -> bool {
    true
}

fn json_error_response(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

fn page_number_does_not_exist_response() -> Response {
    json_error_response(StatusCode::BAD_REQUEST, "Page number does not exist")
}

pub async fn book_page_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    if page_number == 0 {
        return page_number_does_not_exist_response();
    }

    let resolved_book_id = resolve_book_id_for_persisted(&app, &book_id).await;

    if let Ok(Some(media)) = load_persisted_book_media_from_services(&app, &resolved_book_id).await
    {
        if !user_can_access_book_media(&app.reader, &resolved_book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        if !book_media_supports_page_api(&media) {
            return StatusCode::NOT_FOUND.into_response();
        }

        let page_row = match page_resolution::load_book_page_row(
            &app.reader,
            &app.content,
            &resolved_book_id,
            &media,
            page_number as u64,
            true,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) => return page_number_does_not_exist_response(),
            Err(error) => return internal_error_response(error),
        };

        if let Some(bytes) = page_resolution::render_book_page_thumbnail(
            &app.content,
            &media,
            &page_row,
            page_number as u64,
            300,
        )
        .await
        {
            let content_type = "image/jpeg".to_string();

            let etag = asset_etag(bytes.as_slice());
            let last_modified = file_last_modified_header_value(media.file_path.as_path());
            if if_none_match_matches(&headers, etag.as_str()) {
                return asset_not_modified_response(Some(etag.as_str()), last_modified.as_deref());
            }
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(&headers, last_modified)
            {
                return asset_not_modified_response(Some(etag.as_str()), Some(last_modified));
            }

            return asset_ok_response(
                content_type.as_str(),
                bytes,
                Some(etag.as_str()),
                last_modified.as_deref(),
            );
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_pages(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    let resolved_book_id = resolve_book_id_for_persisted(&app, &book_id).await;

    let media = match load_persisted_book_media_from_services(&app, &resolved_book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    if !user_can_access_book_media(&app.reader, &resolved_book_id, &user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    if !book_media_is_ready_status_from_services(&app, &resolved_book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    if !book_media_supports_page_api(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match page_resolution::list_book_page_rows(&app.reader, &app.content, &resolved_book_id, &media)
        .await
    {
        Ok(Some(page_rows)) => page_rows_response(page_rows),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

fn page_rows_response(page_rows: Vec<BookPageRecord>) -> Response {
    Json(
        page_rows
            .into_iter()
            .map(page_row_payload)
            .collect::<Vec<_>>(),
    )
    .into_response()
}

fn page_row_payload(page: BookPageRecord) -> Value {
    let size_bytes = if page.file_size < 0 {
        Value::Null
    } else {
        json!(page.file_size)
    };
    let size = if page.file_size < 0 {
        Value::String(String::new())
    } else {
        Value::String(format_size_bytes(page.file_size as u64))
    };
    json!({
        "number": page.number,
        "fileName": page.file_name,
        "mediaType": page.media_type,
        "width": page.width,
        "height": page.height,
        "sizeBytes": size_bytes,
        "size": size,
    })
}

pub async fn book_positions(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let resolved_book_id = resolve_book_id_for_persisted(&app, &book_id).await;

    let media = match load_persisted_book_media_from_services(&app, &resolved_book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    if !user_can_access_book_media(&app.reader, &resolved_book_id, &user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    if !book_media_is_epub(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let last_modified = file_last_modified_header_value(media.file_path.as_path());

    match load_persisted_epub_positions(&app, &resolved_book_id).await {
        Ok(Some(positions)) if !positions.is_empty() => {
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(&headers, last_modified)
            {
                return asset_not_modified_response(None, Some(last_modified));
            }
            let mut response = Json(json!({
                "total": positions.len(),
                "positions": positions,
            }))
            .into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/vnd.readium.position-list+json"),
            );
            if let Some(last_modified) = last_modified.as_deref() {
                response.headers_mut().insert(
                    header::LAST_MODIFIED,
                    HeaderValue::from_str(last_modified)
                        .expect("positions last-modified header should be valid"),
                );
            }
            response
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
