use super::*;

const MOSAIC_HEIGHT: u32 = 300;
const MOSAIC_RATIO: f32 = 0.70666664;

pub(super) fn thumbnail_dimensions(bytes: &[u8]) -> Option<(i64, i64)> {
    let image = image::load_from_memory(bytes).ok()?;
    Some((i64::from(image.width()), i64::from(image.height())))
}

fn repeated_thumbnail_source_ids(ids: Vec<String>) -> Vec<String> {
    let seed = ids.into_iter().take(4).collect::<Vec<_>>();
    if seed.is_empty() {
        return vec![];
    }

    let mut repeated = Vec::with_capacity(4);
    while repeated.len() < 4 {
        repeated.extend(seed.iter().cloned());
    }
    repeated.truncate(4);
    repeated
}

fn encode_mosaic_jpeg(image_bytes: &[Vec<u8>]) -> Option<Vec<u8>> {
    if image_bytes.is_empty() {
        return None;
    }

    let height = MOSAIC_HEIGHT;
    let width = ((height as f32) * MOSAIC_RATIO).round() as u32;
    let cell_width = (width / 2).max(1);
    let cell_height = (height / 2).max(1);
    let mut mosaic = image::RgbImage::new(width.max(1), height.max(1));
    let placements = [
        (0_i64, 0_i64),
        (i64::from(cell_width), 0_i64),
        (0_i64, i64::from(cell_height)),
        (i64::from(cell_width), i64::from(cell_height)),
    ];

    for (bytes, (x, y)) in image_bytes.iter().zip(placements.into_iter()) {
        let tile = image::load_from_memory(bytes)
            .ok()?
            .thumbnail(cell_width, cell_height)
            .to_rgb8();
        image::imageops::overlay(&mut mosaic, &tile, x, y);
    }

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(mosaic)
        .write_to(&mut output, ImageFormat::Jpeg)
        .ok()?;
    Some(output.into_inner())
}

pub(super) fn encode_image_bytes_as_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Jpeg).ok()?;
    Some(output.into_inner())
}

fn encode_image_bytes_as_small_jpeg(bytes: &[u8], max_edge: u32) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let resized = if image.width().max(image.height()) > max_edge {
        image.resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3)
    } else {
        image
    };
    let mut output = std::io::Cursor::new(Vec::new());
    resized.write_to(&mut output, ImageFormat::Jpeg).ok()?;
    Some(output.into_inner())
}

pub(super) fn response_from_thumbnail_bytes(
    headers: &HeaderMap,
    bytes: Vec<u8>,
    media_type: &str,
) -> Response {
    let etag = asset_etag(bytes.as_slice());
    if if_none_match_matches(headers, etag.as_str()) {
        return asset_not_modified_response(Some(etag.as_str()), None);
    }

    asset_ok_response(media_type, bytes, Some(etag.as_str()), None)
}

pub(super) fn response_from_thumbnail_jpeg_bytes(headers: &HeaderMap, bytes: Vec<u8>) -> Response {
    let Some(jpeg_bytes) = encode_image_bytes_as_jpeg(&bytes) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    response_from_thumbnail_bytes(headers, jpeg_bytes, "image/jpeg")
}

pub(super) fn response_from_thumbnail_small_jpeg_bytes(
    headers: &HeaderMap,
    bytes: Vec<u8>,
    media_type: &str,
    max_edge: u32,
) -> Response {
    match encode_image_bytes_as_small_jpeg(&bytes, max_edge) {
        Some(jpeg_bytes) => response_from_thumbnail_bytes(headers, jpeg_bytes, "image/jpeg"),
        None => response_from_thumbnail_bytes(headers, bytes, media_type),
    }
}

pub(super) fn set_one_hour_private_cache_control(response: &mut Response) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("max-age=3600, private"),
    );
}

pub(super) fn thumbnail_max_edge_from_setting(value: &str) -> u32 {
    match value {
        "MEDIUM" => 600,
        "LARGE" => 900,
        "XLARGE" => 1200,
        _ => 300,
    }
}

pub(super) async fn load_book_thumbnail_source_bytes(
    database_file: &FsPath,
    book_id: &str,
    media: &PersistedBookMedia,
) -> Option<Vec<u8>> {
    if let Ok(Some(thumbnail)) = load_selected_book_thumbnail(database_file, book_id).await
        && thumbnail.thumbnail_type != "GENERATED"
    {
        return Some(thumbnail.thumbnail);
    }

    if book_media_is_epub(media) {
        return load_epub_cover_bytes(media).map(|(bytes, _)| bytes);
    }

    if book_media_is_pdf(media) {
        let page_row = load_persisted_book_page_row(database_file, book_id, 1)
            .await
            .ok()
            .flatten()
            .or_else(|| load_pdf_page_row(media, 1))?;
        return render_book_page_thumbnail(media, &page_row, 1, 300);
    }

    if book_media_is_single_image(media) {
        return read_media_file_bytes(&media.file_path);
    }

    let page_row = load_persisted_book_page_row(database_file, book_id, 1)
        .await
        .ok()
        .flatten()
        .or_else(|| load_archive_page_row(media, 1))?;
    let media_type = if page_row.media_type.is_empty() {
        content_type_from_filename(&page_row.file_name, &media.media_type)
    } else {
        page_row.media_type.clone()
    };
    if !media_type.to_ascii_lowercase().starts_with("image/") {
        return None;
    }

    resolve_book_page_bytes(media, &page_row, 1)
}

pub(super) async fn load_series_thumbnail_source_bytes(
    database_file: &FsPath,
    series_id: &str,
) -> Option<Vec<u8>> {
    if let Ok(Some(thumbnail)) = load_selected_series_thumbnail(database_file, series_id).await {
        return Some(thumbnail.thumbnail);
    }

    let Ok(Some(media)) = load_persisted_series_thumbnail_media(database_file, series_id).await
    else {
        return None;
    };

    read_media_file_bytes(&media.file_path)
}

pub(super) async fn load_readlist_mosaic_bytes(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Option<Vec<u8>>, String> {
    let book_ids = repeated_thumbnail_source_ids(
        load_persisted_readlist_book_rows(database_file, readlist_id)
            .await?
            .into_iter()
            .map(|row| row.book_id)
            .collect(),
    );
    if book_ids.is_empty() {
        return Ok(None);
    }

    let mut images = Vec::new();
    for book_id in book_ids {
        if let Ok(Some(media)) = load_persisted_book_media(database_file, &book_id).await
            && let Some(bytes) =
                load_book_thumbnail_source_bytes(database_file, &book_id, &media).await
        {
            images.push(bytes);
        }
    }

    Ok(encode_mosaic_jpeg(&images))
}

pub(super) async fn load_collection_mosaic_bytes(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Option<Vec<u8>>, String> {
    let series_ids = repeated_thumbnail_source_ids(
        load_persisted_collection_series_ids(database_file, collection_id).await?,
    );
    if series_ids.is_empty() {
        return Ok(None);
    }

    let mut images = Vec::new();
    for series_id in series_ids {
        if let Some(bytes) = load_series_thumbnail_source_bytes(database_file, &series_id).await {
            images.push(bytes);
        }
    }

    Ok(encode_mosaic_jpeg(&images))
}

pub(super) async fn parse_thumbnail_upload(
    mut multipart: Multipart,
    entity_name: &str,
) -> Result<(Vec<u8>, String, bool), Response> {
    let mut image_bytes = None::<Vec<u8>>;
    let mut media_type = None::<String>;
    let mut selected = true;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => return Err(invalid_thumbnail_upload_response(entity_name, error)),
        };

        match field.name() {
            Some("file") => {
                let content_type = field.content_type().map(str::to_string);
                let bytes = match field.bytes().await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return Err(invalid_thumbnail_upload_response(entity_name, error));
                    }
                };
                if bytes.is_empty() {
                    return Err(empty_thumbnail_upload_response(entity_name));
                }

                let resolved_media_type =
                    match resolve_thumbnail_media_type(content_type.as_deref(), bytes.as_ref()) {
                        Some(media_type) => media_type,
                        None => return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response()),
                    };
                image_bytes = Some(bytes.to_vec());
                media_type = Some(resolved_media_type);
            }
            Some("selected") => {
                let value = match field.text().await {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(invalid_thumbnail_upload_response(entity_name, error));
                    }
                };
                selected = match value.trim().to_ascii_lowercase().as_str() {
                    "" | "true" => true,
                    "false" => false,
                    _ => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(json!({
                                "error": format!("{entity_name} thumbnail selected field must be true or false"),
                            })),
                        )
                            .into_response());
                    }
                };
            }
            _ => {}
        }
    }

    let Some(bytes) = image_bytes else {
        return Err(empty_thumbnail_upload_response(entity_name));
    };
    let Some(media_type) = media_type else {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response());
    };

    Ok((bytes, media_type, selected))
}

fn resolve_thumbnail_media_type(content_type: Option<&str>, bytes: &[u8]) -> Option<String> {
    if let Some(content_type) = content_type
        && content_type.starts_with("image/")
    {
        return Some(content_type.to_string());
    }

    match image::guess_format(bytes).ok()? {
        ImageFormat::Jpeg => Some("image/jpeg".to_string()),
        ImageFormat::Png => Some("image/png".to_string()),
        ImageFormat::Gif => Some("image/gif".to_string()),
        ImageFormat::WebP => Some("image/webp".to_string()),
        ImageFormat::Avif => Some("image/avif".to_string()),
        _ => None,
    }
}

fn empty_thumbnail_upload_response(entity_name: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": format!("{entity_name} thumbnail upload body must not be empty"),
        })),
    )
        .into_response()
}

fn invalid_thumbnail_upload_response(entity_name: &str, error: impl std::fmt::Display) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": format!("invalid {entity_name} thumbnail upload: {error}"),
        })),
    )
        .into_response()
}
