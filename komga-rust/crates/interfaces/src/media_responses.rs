use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::ImageFormat;
use serde_json::json;

use crate::cache::{
    asset_etag, asset_not_modified_response, asset_ok_response, file_last_modified_header_value,
    if_modified_since_matches, if_none_match_matches,
};
use crate::identity_access::auth::{AuthUser, user_has_role, user_is_admin};
use crate::media_assets::access_control::user_can_access_book_media;
use crate::media_assets::http_helpers::{
    attachment_disposition, format_size_bytes, inline_disposition, internal_error_response,
};
use crate::media_assets::media_helpers::{
    book_media_is_epub, book_media_is_pdf, book_media_supports_page_api,
};
use crate::media_assets::page_resolution;
use crate::media_assets::thumbnails::shared::{
    response_from_thumbnail_bytes, response_from_thumbnail_jpeg_bytes,
    response_from_thumbnail_small_jpeg_bytes, thumbnail_max_edge_from_setting,
};
use crate::state::{MediaAssetsState, OpdsState};
use komga_application::discovery::BookDetailPort;
use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, ContentResolverPort, MediaReaderPort,
};
use komga_application::operational::ServerSettingsPort;

#[derive(Clone, Debug)]
pub(crate) struct BookPageResponseOptions {
    pub(crate) convert: Option<String>,
    pub(crate) zero_based: bool,
    pub(crate) content_negotiation: bool,
}

impl Default for BookPageResponseOptions {
    fn default() -> Self {
        Self {
            convert: None,
            zero_based: false,
            content_negotiation: true,
        }
    }
}

pub(crate) struct BookMediaResponses<'a> {
    reader: &'a dyn MediaReaderPort,
    content: &'a dyn ContentResolverPort,
    book_detail: &'a dyn BookDetailPort,
}

pub(crate) struct OpdsBookMediaResponses<'a> {
    media: BookMediaResponses<'a>,
    server_settings: &'a dyn ServerSettingsPort,
}

impl<'a> BookMediaResponses<'a> {
    pub(crate) fn for_media_assets(app: &'a MediaAssetsState) -> Self {
        Self {
            reader: app.reader.as_ref(),
            content: app.content.as_ref(),
            book_detail: app.book_detail.as_ref(),
        }
    }

    fn for_opds(app: &'a OpdsState) -> Self {
        Self {
            reader: app.reader.as_ref(),
            content: app.content.as_ref(),
            book_detail: app.book_detail.as_ref(),
        }
    }

    pub(crate) async fn book_file(&self, user: &AuthUser, book_id: &str) -> Response {
        book_file_response(self.reader, self.content, user, book_id).await
    }

    pub(crate) async fn book_page(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number: u32,
        options: BookPageResponseOptions,
    ) -> Response {
        book_page_response(
            self.reader,
            self.content,
            self.book_detail,
            user,
            headers,
            book_id,
            page_number,
            options,
        )
        .await
    }

    pub(crate) async fn book_page_raw(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number_signed: i32,
    ) -> Response {
        book_page_raw_response(
            self.reader,
            self.content,
            self.book_detail,
            user,
            headers,
            book_id,
            page_number_signed,
        )
        .await
    }

    pub(crate) async fn book_page_thumbnail(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number: u32,
    ) -> Response {
        book_page_thumbnail_response(
            self.reader,
            self.content,
            self.book_detail,
            user,
            headers,
            book_id,
            page_number,
        )
        .await
    }

    pub(crate) async fn book_pages(&self, user: &AuthUser, book_id: &str) -> Response {
        book_pages_response(self.reader, self.content, self.book_detail, user, book_id).await
    }
}

impl<'a> OpdsBookMediaResponses<'a> {
    pub(crate) fn new(app: &'a OpdsState) -> Self {
        Self {
            media: BookMediaResponses::for_opds(app),
            server_settings: app.server_settings.as_ref(),
        }
    }

    pub(crate) async fn book_file(&self, user: &AuthUser, book_id: &str) -> Response {
        self.media.book_file(user, book_id).await
    }

    pub(crate) async fn book_page(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number: u32,
        options: BookPageResponseOptions,
    ) -> Response {
        self.media
            .book_page(user, headers, book_id, page_number, options)
            .await
    }

    pub(crate) async fn book_page_raw(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number_signed: i32,
    ) -> Response {
        self.media
            .book_page_raw(user, headers, book_id, page_number_signed)
            .await
    }

    pub(crate) async fn book_thumbnail_opds(
        &self,
        headers: &HeaderMap,
        book_id: &str,
        user: &AuthUser,
    ) -> Response {
        book_thumbnail_opds_response(
            self.media.reader,
            self.media.content,
            headers,
            book_id,
            user,
        )
        .await
    }

    pub(crate) async fn book_thumbnail_opds_small_default(
        &self,
        headers: &HeaderMap,
        book_id: &str,
        user: &AuthUser,
    ) -> Response {
        book_thumbnail_opds_small_default_response(
            self.media.reader,
            self.server_settings,
            headers,
            book_id,
            user,
        )
        .await
    }
}

async fn book_file_response(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    user: &AuthUser,
    book_id: &str,
) -> Response {
    if let Ok(Some(media)) = reader.book_media(book_id).await {
        if !user_can_access_book_media(reader, book_id, user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        let Some(body) = content.read_media_file_bytes(&media.file_path).await else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "File not found, it may have moved" })),
            )
                .into_response();
        };

        let content_type = media.media_type.clone();
        let content_disposition = attachment_disposition(&media.file_name);
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type.as_str()),
                (header::CONTENT_DISPOSITION, content_disposition.as_str()),
            ],
            body,
        )
            .into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

#[allow(clippy::too_many_arguments)]
async fn book_page_response(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    discovery_detail: &dyn BookDetailPort,
    user: &AuthUser,
    headers: &HeaderMap,
    book_id: &str,
    page_number: u32,
    options: BookPageResponseOptions,
) -> Response {
    let resolved_book_id = resolve_book_id_for_persisted(discovery_detail, book_id).await;
    let requested_page_number = if options.zero_based {
        page_number.saturating_add(1)
    } else {
        page_number
    };
    if requested_page_number == 0 {
        return page_number_does_not_exist_response();
    }

    let requested_convert = options
        .convert
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(requested_convert) = requested_convert
        && !matches!(requested_convert, "jpeg" | "png")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if let Ok(Some(media)) = reader.book_media(&resolved_book_id).await {
        let last_modified = file_last_modified_header_value(media.file_path.as_path());
        if let Some(last_modified) = last_modified.as_deref()
            && if_modified_since_matches(headers, last_modified)
        {
            return asset_not_modified_response(None, Some(last_modified));
        }

        let book_display_name = media.file_name.clone();
        if !reader
            .book_media_is_ready(&resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Book analysis failed" })),
            )
                .into_response();
        }

        if !user_is_admin(user) && !user_has_role(user, "PAGE_STREAMING") {
            return StatusCode::FORBIDDEN.into_response();
        }
        if !user_can_access_book_media(reader, &resolved_book_id, user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }
        if !book_media_supports_page_api(&media) {
            return StatusCode::NOT_FOUND.into_response();
        }

        if book_media_is_pdf(&media)
            && options.content_negotiation
            && accept_header_prefers_pdf(headers)
        {
            let page_count = content
                .detect_pdf_page_count(&media)
                .unwrap_or(media.page_count);
            if requested_page_number as u64 > page_count {
                return page_number_does_not_exist_response();
            }
            if let Some(bytes) =
                content.read_pdf_page_as_single_page_pdf(&media, requested_page_number as u64)
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

        let page_row = match page_resolution::load_book_page_row(
            reader,
            content,
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

        if let Some(bytes) = content
            .resolve_page_bytes(&media, &page_row, requested_page_number as u64)
            .await
        {
            let mut effective_bytes = bytes;
            let content_type = page_resolution::page_row_media_type(&page_row, &media);
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

async fn book_page_raw_response(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    discovery_detail: &dyn BookDetailPort,
    user: &AuthUser,
    headers: &HeaderMap,
    book_id: &str,
    page_number_signed: i32,
) -> Response {
    if page_number_signed <= 0 {
        return json_error_response(StatusCode::BAD_REQUEST, "Page number does not exist");
    }
    let page_number = page_number_signed as u32;
    let resolved_book_id = resolve_book_id_for_persisted(discovery_detail, book_id).await;

    if let Ok(Some(media)) = reader.book_media(&resolved_book_id).await {
        if !user_has_role(user, "PAGE_STREAMING") {
            return StatusCode::FORBIDDEN.into_response();
        }

        let last_modified = file_last_modified_header_value(media.file_path.as_path());
        if let Some(last_modified) = last_modified.as_deref()
            && if_modified_since_matches(headers, last_modified)
        {
            return asset_not_modified_response(None, Some(last_modified));
        }

        let book_display_name = media.file_name.clone();
        if !user_can_access_book_media(reader, &resolved_book_id, user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }
        if !book_media_is_pdf(&media) {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "Extractor does not support raw extraction of pages",
            );
        }
        if !reader
            .book_media_is_ready(&resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return json_error_response(StatusCode::NOT_FOUND, "Book analysis failed");
        }
        if !media.file_path.exists() {
            return json_error_response(StatusCode::NOT_FOUND, "File not found, it may have moved");
        }

        let page_count = content
            .detect_pdf_page_count(&media)
            .unwrap_or(media.page_count);
        if page_number == 0 || page_number as u64 > page_count {
            return page_number_does_not_exist_response();
        }

        if let Some(bytes) = content.read_pdf_page_as_single_page_pdf(&media, page_number as u64) {
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

async fn book_page_thumbnail_response(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    discovery_detail: &dyn BookDetailPort,
    user: &AuthUser,
    headers: &HeaderMap,
    book_id: &str,
    page_number: u32,
) -> Response {
    if page_number == 0 {
        return page_number_does_not_exist_response();
    }

    let resolved_book_id = resolve_book_id_for_persisted(discovery_detail, book_id).await;

    if let Ok(Some(media)) = reader.book_media(&resolved_book_id).await {
        if !user_can_access_book_media(reader, &resolved_book_id, user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        if !book_media_supports_page_api(&media) {
            return StatusCode::NOT_FOUND.into_response();
        }

        let page_row = match page_resolution::load_book_page_row(
            reader,
            content,
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
            content,
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
            if if_none_match_matches(headers, etag.as_str()) {
                return asset_not_modified_response(Some(etag.as_str()), last_modified.as_deref());
            }
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(headers, last_modified)
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

async fn book_pages_response(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    discovery_detail: &dyn BookDetailPort,
    user: &AuthUser,
    book_id: &str,
) -> Response {
    let resolved_book_id = resolve_book_id_for_persisted(discovery_detail, book_id).await;

    let media = match reader.book_media(&resolved_book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    if !user_can_access_book_media(reader, &resolved_book_id, user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    if !reader
        .book_media_is_ready(&resolved_book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    if !book_media_supports_page_api(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match page_resolution::list_book_page_rows(reader, content, &resolved_book_id, &media).await {
        Ok(Some(page_rows)) => page_rows_response(page_rows),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

async fn book_thumbnail_opds_response(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    headers: &HeaderMap,
    book_id: &str,
    user: &AuthUser,
) -> Response {
    if let Ok(Some(media)) = reader.book_media(book_id).await {
        if !user_can_access_book_media(reader, book_id, user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        if let Some(bytes) =
            load_book_thumbnail_source_bytes(reader, content, book_id, &media).await
        {
            return response_from_thumbnail_jpeg_bytes(headers, bytes);
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn book_thumbnail_opds_small_default_response(
    reader: &dyn MediaReaderPort,
    server_settings: &dyn ServerSettingsPort,
    headers: &HeaderMap,
    book_id: &str,
    user: &AuthUser,
) -> Response {
    let settings = match server_settings.load_settings().await {
        Ok(settings) => settings,
        Err(error) => return internal_error_response(error),
    };

    book_thumbnail_opds_small_response(
        reader,
        headers,
        book_id,
        thumbnail_max_edge_from_setting(settings.thumbnail_size),
        user,
    )
    .await
}

async fn book_thumbnail_opds_small_response(
    reader: &dyn MediaReaderPort,
    headers: &HeaderMap,
    book_id: &str,
    max_edge: u32,
    user: &AuthUser,
) -> Response {
    if let Ok(Some(media)) = reader.book_media(book_id).await {
        if !user_can_access_book_media(reader, book_id, user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        match reader.selected_book_thumbnail(book_id).await {
            Ok(Some(thumbnail)) => {
                if thumbnail.thumbnail_type == "GENERATED" {
                    return response_from_thumbnail_bytes(
                        headers,
                        thumbnail.thumbnail,
                        thumbnail.media_type.as_str(),
                    );
                }

                return response_from_thumbnail_small_jpeg_bytes(
                    headers,
                    thumbnail.thumbnail,
                    thumbnail.media_type.as_str(),
                    max_edge,
                );
            }
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn resolve_book_id_for_persisted(
    discovery_detail: &dyn BookDetailPort,
    requested_book_id: &str,
) -> String {
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };

    if index == 0 {
        return requested_book_id.to_string();
    }

    if matches!(
        discovery_detail
            .load_persisted_book_resource(requested_book_id)
            .await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match discovery_detail
        .load_book_id_by_sorted_position(index)
        .await
    {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

async fn load_book_thumbnail_source_bytes(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    book_id: &str,
    media: &BookMediaRecord,
) -> Option<Vec<u8>> {
    if let Ok(Some(thumbnail)) = reader.selected_book_thumbnail(book_id).await
        && thumbnail.thumbnail_type != "GENERATED"
    {
        return Some(thumbnail.thumbnail);
    }

    if book_media_is_epub(media) {
        return content
            .epub_cover_bytes(media)
            .await
            .map(|(bytes, _)| bytes);
    }

    page_resolution::load_book_thumbnail_page_source_bytes(reader, content, book_id, media).await
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

fn page_rows_response(page_rows: Vec<BookPageRecord>) -> Response {
    Json(
        page_rows
            .into_iter()
            .map(page_row_payload)
            .collect::<Vec<_>>(),
    )
    .into_response()
}

fn page_row_payload(page: BookPageRecord) -> serde_json::Value {
    let size_bytes = if page.file_size < 0 {
        serde_json::Value::Null
    } else {
        json!(page.file_size)
    };
    let size = if page.file_size < 0 {
        serde_json::Value::String(String::new())
    } else {
        serde_json::Value::String(format_size_bytes(page.file_size as u64))
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
