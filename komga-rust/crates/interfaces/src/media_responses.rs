use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::cache::{
    asset_not_modified_response, file_last_modified_header_value, if_modified_since_matches,
};
use crate::identity_access::auth::{AuthUser, user_has_role};
use crate::media_assets::http_helpers::{
    attachment_disposition, format_size_bytes, inline_disposition, internal_error_response,
};
use crate::media_assets::thumbnails::shared::{
    response_from_thumbnail_bytes, response_from_thumbnail_jpeg_bytes,
    response_from_thumbnail_small_jpeg_bytes, thumbnail_max_edge_from_setting,
};
use crate::media_response_policy::MediaAssetResponse;
use crate::state::{MediaAssetsState, OpdsState};
use komga_application::discovery::BookDetailPort;
use komga_application::media_assets::{
    BookMediaDelivery, BookMediaDeliveryAsset, BookMediaDeliveryDisposition,
    BookMediaDeliveryService, BookMediaPageRequest, BookPageRecord, BookThumbnailDelivery,
    ContentResolverPort, MediaReaderPort,
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
        let service = self.delivery_service();
        book_file_delivery_response(service.book_file(user, book_id).await)
    }

    pub(crate) async fn book_page(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number: u32,
        options: BookPageResponseOptions,
    ) -> Response {
        let request = BookMediaPageRequest {
            convert: options.convert,
            zero_based: options.zero_based,
            prefer_pdf: options.content_negotiation && accept_header_prefers_pdf(headers),
        };
        let service = self.delivery_service();
        book_page_delivery_response(
            headers,
            service.book_page(user, book_id, page_number, request).await,
        )
    }

    pub(crate) async fn book_page_raw(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number_signed: i32,
    ) -> Response {
        if let Some(response) = self
            .raw_page_not_modified_response(user, headers, book_id, page_number_signed)
            .await
        {
            return response;
        }

        let service = self.delivery_service();
        book_page_delivery_response(
            headers,
            service
                .book_page_raw(user, book_id, page_number_signed)
                .await,
        )
    }

    pub(crate) async fn book_page_thumbnail(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number: u32,
    ) -> Response {
        let service = self.delivery_service();
        book_page_thumbnail_delivery_response(
            headers,
            service
                .book_page_thumbnail(user, book_id, page_number)
                .await,
        )
    }

    pub(crate) async fn book_pages(&self, user: &AuthUser, book_id: &str) -> Response {
        let service = self.delivery_service();
        book_pages_delivery_response(service.book_pages(user, book_id).await)
    }

    async fn book_thumbnail_source(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
    ) -> Response {
        let service = self.delivery_service();
        book_thumbnail_source_delivery_response(
            headers,
            service.book_thumbnail_source(user, book_id).await,
        )
    }

    async fn selected_book_thumbnail_small(
        &self,
        headers: &HeaderMap,
        book_id: &str,
        max_edge: u32,
        user: &AuthUser,
    ) -> Response {
        let service = self.delivery_service();
        selected_thumbnail_small_delivery_response(
            headers,
            service.selected_book_thumbnail(user, book_id).await,
            max_edge,
        )
    }

    fn delivery_service(
        &self,
    ) -> BookMediaDeliveryService<
        '_,
        dyn MediaReaderPort + 'a,
        dyn ContentResolverPort + 'a,
        dyn BookDetailPort + 'a,
    > {
        BookMediaDeliveryService::new(self.reader, self.content, self.book_detail)
    }

    async fn raw_page_not_modified_response(
        &self,
        user: &AuthUser,
        headers: &HeaderMap,
        book_id: &str,
        page_number_signed: i32,
    ) -> Option<Response> {
        if page_number_signed <= 0 || !user_has_role(user, "PAGE_STREAMING") {
            return None;
        }

        let resolved_book_id =
            resolve_book_id_for_conditional_raw_page(self.book_detail, book_id).await;
        let media = self
            .reader
            .book_media(&resolved_book_id)
            .await
            .ok()
            .flatten()?;
        let last_modified = file_last_modified_header_value(media.file_path.as_path())?;
        if !if_modified_since_matches(headers, last_modified.as_str()) {
            return None;
        }

        Some(asset_not_modified_response(
            None,
            Some(last_modified.as_str()),
        ))
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
        self.media
            .book_thumbnail_source(user, headers, book_id)
            .await
    }

    pub(crate) async fn book_thumbnail_opds_small_default(
        &self,
        headers: &HeaderMap,
        book_id: &str,
        user: &AuthUser,
    ) -> Response {
        let settings = match self.server_settings.load_settings().await {
            Ok(settings) => settings,
            Err(error) => return internal_error_response(error),
        };

        self.media
            .selected_book_thumbnail_small(
                headers,
                book_id,
                thumbnail_max_edge_from_setting(settings.thumbnail_size),
                user,
            )
            .await
    }
}

async fn resolve_book_id_for_conditional_raw_page(
    book_detail: &dyn BookDetailPort,
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
        book_detail
            .load_persisted_book_resource(requested_book_id)
            .await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match book_detail.load_book_id_by_sorted_position(index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

fn book_file_delivery_response(delivery: BookMediaDelivery) -> Response {
    match delivery {
        BookMediaDelivery::Asset(asset) => asset_response(None, asset, false, false),
        BookMediaDelivery::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookMediaDelivery::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookMediaDelivery::MissingFile => {
            json_error_response(StatusCode::NOT_FOUND, "File not found, it may have moved")
        }
        BookMediaDelivery::Internal(error) => internal_error_response(error),
        BookMediaDelivery::MediaAnalysisFailed
        | BookMediaDelivery::BadRequest(_)
        | BookMediaDelivery::Pages(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn book_page_delivery_response(headers: &HeaderMap, delivery: BookMediaDelivery) -> Response {
    match delivery {
        BookMediaDelivery::Asset(asset) => asset_response(Some(headers), asset, false, true),
        BookMediaDelivery::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookMediaDelivery::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookMediaDelivery::MediaAnalysisFailed => {
            json_error_response(StatusCode::NOT_FOUND, "Book analysis failed")
        }
        BookMediaDelivery::MissingFile => {
            json_error_response(StatusCode::NOT_FOUND, "File not found, it may have moved")
        }
        BookMediaDelivery::BadRequest(Some(error)) => {
            json_error_response(StatusCode::BAD_REQUEST, &error)
        }
        BookMediaDelivery::BadRequest(None) => StatusCode::BAD_REQUEST.into_response(),
        BookMediaDelivery::Internal(error) => internal_error_response(error),
        BookMediaDelivery::Pages(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn book_page_thumbnail_delivery_response(
    headers: &HeaderMap,
    delivery: BookMediaDelivery,
) -> Response {
    match delivery {
        BookMediaDelivery::Asset(asset) => asset_response(Some(headers), asset, true, true),
        BookMediaDelivery::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookMediaDelivery::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookMediaDelivery::BadRequest(Some(error)) => {
            json_error_response(StatusCode::BAD_REQUEST, &error)
        }
        BookMediaDelivery::BadRequest(None) => StatusCode::BAD_REQUEST.into_response(),
        BookMediaDelivery::Internal(error) => internal_error_response(error),
        BookMediaDelivery::MediaAnalysisFailed
        | BookMediaDelivery::MissingFile
        | BookMediaDelivery::Pages(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn book_pages_delivery_response(delivery: BookMediaDelivery) -> Response {
    match delivery {
        BookMediaDelivery::Pages(page_rows) => page_rows_response(page_rows),
        BookMediaDelivery::NotFound | BookMediaDelivery::MediaAnalysisFailed => {
            StatusCode::NOT_FOUND.into_response()
        }
        BookMediaDelivery::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookMediaDelivery::Internal(error) => internal_error_response(error),
        BookMediaDelivery::Asset(_)
        | BookMediaDelivery::MissingFile
        | BookMediaDelivery::BadRequest(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn book_thumbnail_source_delivery_response(
    headers: &HeaderMap,
    delivery: BookThumbnailDelivery,
) -> Response {
    match delivery {
        BookThumbnailDelivery::Thumbnail(thumbnail) => {
            response_from_thumbnail_jpeg_bytes(headers, thumbnail.bytes)
        }
        BookThumbnailDelivery::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookThumbnailDelivery::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookThumbnailDelivery::Internal(error) => internal_error_response(error),
    }
}

fn selected_thumbnail_small_delivery_response(
    headers: &HeaderMap,
    delivery: BookThumbnailDelivery,
    max_edge: u32,
) -> Response {
    match delivery {
        BookThumbnailDelivery::Thumbnail(thumbnail) if thumbnail.generated => {
            response_from_thumbnail_bytes(headers, thumbnail.bytes, thumbnail.media_type.as_str())
        }
        BookThumbnailDelivery::Thumbnail(thumbnail) => response_from_thumbnail_small_jpeg_bytes(
            headers,
            thumbnail.bytes,
            thumbnail.media_type.as_str(),
            max_edge,
        ),
        BookThumbnailDelivery::NotFound => StatusCode::NOT_FOUND.into_response(),
        BookThumbnailDelivery::Forbidden => StatusCode::FORBIDDEN.into_response(),
        BookThumbnailDelivery::Internal(error) => internal_error_response(error),
    }
}

fn asset_response(
    headers: Option<&HeaderMap>,
    asset: BookMediaDeliveryAsset,
    include_etag: bool,
    include_last_modified: bool,
) -> Response {
    let last_modified = include_last_modified
        .then(|| {
            asset
                .source_file
                .as_deref()
                .and_then(file_last_modified_header_value)
        })
        .flatten();

    let mut response = MediaAssetResponse::new(asset.content_type, asset.bytes);
    if include_etag {
        response = response.with_etag();
    }

    response
        .with_last_modified(last_modified)
        .with_content_disposition(content_disposition(
            asset.disposition,
            asset.file_name.as_deref(),
        ))
        .into_response(headers)
}

fn content_disposition(
    disposition: BookMediaDeliveryDisposition,
    file_name: Option<&str>,
) -> Option<String> {
    let file_name = file_name?;
    match disposition {
        BookMediaDeliveryDisposition::Attachment => Some(attachment_disposition(file_name)),
        BookMediaDeliveryDisposition::Inline => Some(inline_disposition(file_name)),
        BookMediaDeliveryDisposition::None => None,
    }
}

fn json_error_response(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
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
