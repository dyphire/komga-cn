use super::*;
use crate::identity_access::auth::Authenticated;
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
        app.book_access.as_ref(),
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
        app.book_access.as_ref(),
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
        app.book_access.as_ref(),
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

async fn single_image_page_row(
    app: &MediaAssetsState,
    media: &PersistedBookMedia,
    page_number: u64,
) -> PersistedBookPageRow {
    let (width, height) = read_media_image_dimensions(app, media.file_path.as_path())
        .await
        .map(|(width, height)| (Some(width), Some(height)))
        .unwrap_or((None, None));
    PersistedBookPageRow {
        number: page_number,
        file_name: media.file_name.clone(),
        media_type: content_type_from_filename(&media.file_name, &media.media_type),
        width,
        height,
        file_size: read_media_file_size_from_services(app, &media.file_path)
            .await
            .unwrap_or(0),
    }
}

async fn single_image_page_record(
    app: &MediaAssetsState,
    media: &PersistedBookMedia,
) -> BookPageRecord {
    let page = single_image_page_row(app, media, 1).await;
    BookPageRecord {
        number: page.number,
        file_name: page.file_name,
        media_type: page.media_type,
        width: page.width,
        height: page.height,
        file_size: page.file_size,
    }
}

async fn load_page_row(
    app: &MediaAssetsState,
    book_id: &str,
    media: &PersistedBookMedia,
    page_number: u64,
    allow_pdf_fallback: bool,
) -> Result<Option<PersistedBookPageRow>, String> {
    match load_persisted_book_page_row_from_services(app, book_id, page_number).await {
        Ok(Some(row)) => Ok(Some(row)),
        Ok(None) if book_media_is_single_image(media) && page_number == 1 => {
            Ok(Some(single_image_page_row(app, media, page_number).await))
        }
        Ok(None) => {
            if let Some(row) = load_archive_page_row_from_services(app, media, page_number).await {
                return Ok(Some(row));
            }
            if allow_pdf_fallback {
                return Ok(load_pdf_page_row_from_services(app, media, page_number));
            }
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

async fn read_media_image_dimensions(app: &MediaAssetsState, path: &FsPath) -> Option<(i64, i64)> {
    let bytes = read_media_file_bytes_from_services(app, path).await?;
    let image = image::load_from_memory(&bytes).ok()?;
    Some((i64::from(image.width()), i64::from(image.height())))
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

        let page_row =
            match load_page_row(&app, &resolved_book_id, &media, page_number as u64, true).await {
                Ok(Some(row)) => row,
                Ok(None) => return page_number_does_not_exist_response(),
                Err(error) => return internal_error_response(error),
            };

        if let Some(bytes) = render_book_page_thumbnail_from_services(
            &app,
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

    let page_rows = match load_persisted_book_pages_from_services(&app, &resolved_book_id).await {
        Ok(rows) => rows,
        Err(error) => return internal_error_response(error),
    };

    if !page_rows.is_empty() {
        let page_rows = if book_media_is_pdf(&media) {
            map_kotlin_pdf_pages(page_rows)
        } else {
            page_rows
        };
        return page_rows_response(page_rows);
    }

    if let Some(archive_rows) = load_archive_page_rows_from_services(&app, &media).await
        && !archive_rows.is_empty()
    {
        return page_rows_response(archive_rows);
    }

    let generated_pdf_rows = load_generated_pdf_page_rows_from_services(&app, &media);
    if !generated_pdf_rows.is_empty() {
        return page_rows_response(generated_pdf_rows);
    }

    if !book_media_is_single_image(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    page_rows_response(vec![single_image_page_record(&app, &media).await])
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

fn map_kotlin_pdf_pages(page_rows: Vec<BookPageRecord>) -> Vec<BookPageRecord> {
    page_rows
        .into_iter()
        .map(|page| {
            let (width, height) = scale_pdf_dimensions(page.width, page.height);
            BookPageRecord {
                media_type: "image/jpeg".to_string(),
                width,
                height,
                ..page
            }
        })
        .collect()
}

fn scale_pdf_dimensions(width: Option<i64>, height: Option<i64>) -> (Option<i64>, Option<i64>) {
    const PDF_RESOLUTION: f64 = 3200.0;

    let (Some(width), Some(height)) = (width, height) else {
        return (None, None);
    };
    let min_edge = width.min(height);
    if min_edge <= 0 {
        return (Some(width), Some(height));
    }

    let scale = PDF_RESOLUTION / min_edge as f64;
    let scaled_width = (width as f64 * scale).round().max(1.0) as i64;
    let scaled_height = (height as f64 * scale).round().max(1.0) as i64;
    (Some(scaled_width), Some(scaled_height))
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
