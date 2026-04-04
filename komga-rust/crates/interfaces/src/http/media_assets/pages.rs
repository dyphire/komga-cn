use super::*;
use komga_application::media_assets::BookPageRecord;

pub async fn book_page(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    book_page_response(&auth_db, &headers, &book_id, page_number, query).await
}

pub async fn book_page_opds_v1(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Query(mut query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    query.zero_based = true;
    query.content_negotiation = false;
    book_page_response(&auth_db, &headers, &book_id, page_number, query).await
}

async fn book_page_response(
    auth_db: &AuthDatabaseState,
    headers: &HeaderMap,
    book_id: &str,
    page_number: u32,
    query: BookPageQuery,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id = resolve_book_id_for_persisted(auth_db.database_file.as_path(), book_id).await;

    let requested_page_number = if query.zero_based {
        page_number.saturating_add(1)
    } else {
        page_number
    };
    let requested_convert = query
        .convert
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let content_negotiation = query.content_negotiation;

    if let Some(requested_convert) = requested_convert
        && !matches!(requested_convert, "jpeg" | "png")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await
    {
        let last_modified = file_last_modified_header_value(media.file_path.as_path());
        if let Some(last_modified) = last_modified.as_deref()
            && if_modified_since_matches(&headers, last_modified)
        {
            return asset_not_modified_response(None, Some(last_modified));
        }

        let book_display_name = media.file_name.clone();

        if !book_media_is_ready_status(auth_db.database_file.as_path(), &resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Book analysis failed" })),
            )
                .into_response();
        }

        if let Some(user) = resolved_auth_user(headers) {
            if !user_is_admin(&user) && !user_has_role(&user, "PAGE_STREAMING") {
                return StatusCode::FORBIDDEN.into_response();
            }
            if !user_can_access_book_media(
                auth_db.database_file.as_path(),
                &resolved_book_id,
                &user,
                &media,
            )
            .await
            {
                return StatusCode::FORBIDDEN.into_response();
            }
        }

        if !book_media_supports_page_api(&media) {
            return StatusCode::NOT_FOUND.into_response();
        }

        if book_media_is_pdf(&media) && content_negotiation && accept_header_prefers_pdf(headers) {
            if requested_page_number == 0 {
                return page_number_does_not_exist_response();
            }
            let page_count = detect_pdf_page_count(&media).unwrap_or(media.page_count);
            if requested_page_number as u64 > page_count {
                return page_number_does_not_exist_response();
            }
            if let Some(bytes) =
                read_pdf_page_as_single_page_pdf(&media, requested_page_number as u64)
            {
                let last_modified = file_last_modified_header_value(media.file_path.as_path());
                if let Some(last_modified) = last_modified.as_deref()
                    && if_modified_since_matches(headers, last_modified)
                {
                    return asset_not_modified_response(None, Some(last_modified));
                }

                return asset_ok_with_inline_disposition(
                    &book_display_name,
                    requested_page_number,
                    "application/pdf",
                    bytes,
                    None,
                    last_modified.as_deref(),
                );
            }
            return page_number_does_not_exist_response();
        }

        let page_row = match load_page_row(
            auth_db.database_file.as_path(),
            &resolved_book_id,
            &media,
            requested_page_number as u64,
            true,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) => return page_number_does_not_exist_response(),
            Err(error) => return internal_error_response(error),
        };

        if let Some(bytes) =
            resolve_book_page_bytes(&media, &page_row, requested_page_number as u64)
        {
            let mut effective_bytes = bytes;
            let content_type = page_row_media_type(&page_row, &media);

            let mut effective_content_type = content_type;
            if let Some(requested_convert) = requested_convert {
                let target_content_type = match requested_convert {
                    "jpeg" => "image/jpeg",
                    "png" => "image/png",
                    _ => unreachable!("validated convert query should be jpeg|png"),
                };

                let Some(converted) = convert_page_image_bytes(
                    &effective_bytes,
                    &effective_content_type,
                    target_content_type,
                ) else {
                    return StatusCode::NOT_FOUND.into_response();
                };
                effective_bytes = converted;
                effective_content_type = target_content_type.to_string();
            }

            let last_modified = file_last_modified_header_value(media.file_path.as_path());
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(headers, last_modified)
            {
                return asset_not_modified_response(None, Some(last_modified));
            }

            return asset_ok_with_inline_disposition(
                &book_display_name,
                requested_page_number,
                effective_content_type.as_str(),
                effective_bytes,
                None,
                last_modified.as_deref(),
            );
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_page_raw(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, page_number_signed)): Path<(String, i32)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }
    if page_number_signed <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Page number does not exist" })),
        )
            .into_response();
    }
    let page_number = page_number_signed as u32;

    let resolved_book_id =
        resolve_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await
    {
        let auth_user = resolved_auth_user(&headers);
        if let Some(user) = auth_user.as_ref()
            && !user_has_role(user, "PAGE_STREAMING")
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        let last_modified = file_last_modified_header_value(media.file_path.as_path());
        if let Some(last_modified) = last_modified.as_deref()
            && if_modified_since_matches(&headers, last_modified)
        {
            return asset_not_modified_response(None, Some(last_modified));
        }

        let book_display_name = media.file_name.clone();

        if let Some(user) = auth_user.as_ref() {
            if !user_can_access_book_media(
                auth_db.database_file.as_path(),
                &resolved_book_id,
                user,
                &media,
            )
            .await
            {
                return StatusCode::FORBIDDEN.into_response();
            }
        }

        if !book_media_is_ready_status(auth_db.database_file.as_path(), &resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return json_error_response(StatusCode::NOT_FOUND, "Book analysis failed");
        }

        if !media.file_path.exists() {
            return json_error_response(StatusCode::NOT_FOUND, "File not found, it may have moved");
        }

        if !book_media_is_pdf(&media) {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "Extractor does not support raw extraction of pages",
            );
        }

        let page_count = detect_pdf_page_count(&media).unwrap_or(media.page_count);
        if page_number == 0 || page_number as u64 > page_count {
            return page_number_does_not_exist_response();
        }

        if let Some(bytes) = read_pdf_page_as_single_page_pdf(&media, page_number as u64) {
            return asset_ok_with_inline_disposition(
                &book_display_name,
                page_number,
                "application/pdf",
                bytes,
                None,
                last_modified.as_deref(),
            );
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

#[derive(Deserialize, Default)]
pub struct BookPageQuery {
    #[serde(default)]
    convert: Option<String>,

    #[serde(default)]
    zero_based: bool,

    #[serde(default = "book_page_content_negotiation_default")]
    #[serde(rename = "contentNegotiation")]
    content_negotiation: bool,
}

fn book_page_content_negotiation_default() -> bool {
    true
}

fn page_response_file_name(book_display_name: &str, page_number: u32, media_type: &str) -> String {
    let extension = mime_guess::get_mime_extensions_str(media_type)
        .and_then(|extensions| extensions.first().copied())
        .unwrap_or("bin");
    format!("{book_display_name}-{page_number}.{extension}")
}

fn json_error_response(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

fn page_number_does_not_exist_response() -> Response {
    json_error_response(StatusCode::BAD_REQUEST, "Page number does not exist")
}

fn single_image_page_row(media: &PersistedBookMedia, page_number: u64) -> PersistedBookPageRow {
    let (width, height) = read_media_image_dimensions(media.file_path.as_path())
        .map(|(width, height)| (Some(width), Some(height)))
        .unwrap_or((None, None));
    PersistedBookPageRow {
        number: page_number,
        file_name: media.file_name.clone(),
        media_type: content_type_from_filename(&media.file_name, &media.media_type),
        width,
        height,
        file_size: read_media_file_size(&media.file_path).unwrap_or(0),
    }
}

fn single_image_page_record(media: &PersistedBookMedia) -> BookPageRecord {
    let page = single_image_page_row(media, 1);
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
    database_file: &FsPath,
    book_id: &str,
    media: &PersistedBookMedia,
    page_number: u64,
    allow_pdf_fallback: bool,
) -> Result<Option<PersistedBookPageRow>, String> {
    match load_persisted_book_page_row(database_file, book_id, page_number).await {
        Ok(Some(row)) => Ok(Some(row)),
        Ok(None) if book_media_is_single_image(media) && page_number == 1 => {
            Ok(Some(single_image_page_row(media, page_number)))
        }
        Ok(None) => {
            if let Some(row) = load_archive_page_row(media, page_number) {
                return Ok(Some(row));
            }
            if allow_pdf_fallback {
                return Ok(load_pdf_page_row(media, page_number));
            }
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn page_row_media_type(page_row: &PersistedBookPageRow, media: &PersistedBookMedia) -> String {
    if page_row.media_type.is_empty() {
        content_type_from_filename(&page_row.file_name, &media.media_type)
    } else {
        page_row.media_type.clone()
    }
}

fn asset_ok_with_inline_disposition(
    book_display_name: &str,
    page_number: u32,
    media_type: &str,
    bytes: Vec<u8>,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Response {
    let mut response = asset_ok_response(media_type, bytes, etag, last_modified);
    let file_name = page_response_file_name(book_display_name, page_number, media_type);
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&inline_disposition(&file_name))
            .expect("page content disposition should be valid"),
    );
    response
}

fn accept_header_prefers_pdf(headers: &HeaderMap) -> bool {
    let Some(raw) = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };

    #[derive(Clone, Copy)]
    struct Candidate {
        rank: i32,
        quality: f32,
        is_pdf: bool,
    }

    fn parse_quality(params: &str) -> f32 {
        for part in params.split(';') {
            let part = part.trim();
            if let Some(value) = part.strip_prefix("q=")
                && let Ok(parsed) = value.parse::<f32>()
            {
                return parsed.clamp(0.0, 1.0);
            }
        }
        1.0
    }

    let mut best: Option<Candidate> = None;
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        let mut parts = entry.split(';');
        let media_type = parts
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let params = parts.collect::<Vec<_>>().join(";");
        let quality = parse_quality(&params);
        if quality <= 0.0 {
            continue;
        }

        let candidate = if media_type == "application/pdf" {
            Some(Candidate {
                rank: 3,
                quality,
                is_pdf: true,
            })
        } else if media_type.starts_with("image/") && media_type != "image/*" {
            Some(Candidate {
                rank: 3,
                quality,
                is_pdf: false,
            })
        } else if media_type == "image/*" {
            Some(Candidate {
                rank: 2,
                quality,
                is_pdf: false,
            })
        } else if media_type == "*/*" {
            Some(Candidate {
                rank: 1,
                quality,
                is_pdf: false,
            })
        } else {
            None
        };

        let Some(candidate) = candidate else {
            continue;
        };
        let replace = match best {
            None => true,
            Some(current) => {
                candidate.rank > current.rank
                    || (candidate.rank == current.rank && candidate.quality > current.quality)
            }
        };
        if replace {
            best = Some(candidate);
        }
    }

    best.map(|candidate| candidate.is_pdf).unwrap_or(false)
}

fn convert_page_image_bytes(
    bytes: &[u8],
    source_content_type: &str,
    target_content_type: &str,
) -> Option<Vec<u8>> {
    if source_content_type.eq_ignore_ascii_case(target_content_type) {
        return Some(bytes.to_vec());
    }

    if !source_content_type
        .to_ascii_lowercase()
        .starts_with("image/")
    {
        return None;
    }

    let source = image::load_from_memory(bytes).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    let target_format = match target_content_type {
        "image/jpeg" => ImageFormat::Jpeg,
        "image/png" => ImageFormat::Png,
        _ => return None,
    };
    source.write_to(&mut output, target_format).ok()?;
    Some(output.into_inner())
}

fn read_media_image_dimensions(path: &FsPath) -> Option<(i64, i64)> {
    let bytes = read_media_file_bytes(path)?;
    let image = image::load_from_memory(&bytes).ok()?;
    Some((i64::from(image.width()), i64::from(image.height())))
}

pub async fn book_page_thumbnail(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id =
        resolve_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_book_media(
                auth_db.database_file.as_path(),
                &resolved_book_id,
                &user,
                &media,
            )
            .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        if !book_media_supports_page_api(&media) {
            return StatusCode::NOT_FOUND.into_response();
        }

        let page_row = match load_page_row(
            auth_db.database_file.as_path(),
            &resolved_book_id,
            &media,
            page_number as u64,
            false,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        };

        if let Some(bytes) = resolve_book_page_bytes(&media, &page_row, page_number as u64) {
            let content_type = page_row_media_type(&page_row, &media);

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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id =
        resolve_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    let media =
        match load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        };

    if let Some(user) = resolved_auth_user(&headers)
        && !user_can_access_book_media(
            auth_db.database_file.as_path(),
            &resolved_book_id,
            &user,
            &media,
        )
        .await
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    if !book_media_is_ready_status(auth_db.database_file.as_path(), &resolved_book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    if !book_media_supports_page_api(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let page_rows =
        match load_persisted_book_pages(auth_db.database_file.as_path(), &resolved_book_id).await {
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

    if let Some(archive_rows) = load_archive_page_rows(&media)
        && !archive_rows.is_empty()
    {
        return page_rows_response(archive_rows);
    }

    let generated_pdf_rows = load_generated_pdf_page_rows(&media);
    if !generated_pdf_rows.is_empty() {
        return page_rows_response(generated_pdf_rows);
    }

    if !book_media_is_single_image(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    page_rows_response(vec![single_image_page_record(&media)])
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
    Extension(_state): Extension<OperationalState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id =
        resolve_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

    let media =
        match load_persisted_book_media(auth_db.database_file.as_path(), &resolved_book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        };

    if let Some(user) = resolved_auth_user(&headers)
        && !user_can_access_library(&user, &media.library_id)
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    if !book_media_is_epub(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let last_modified = file_last_modified_header_value(media.file_path.as_path());

    match load_persisted_epub_positions(auth_db.database_file.as_path(), &resolved_book_id).await {
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
            return response;
        }
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }
}
