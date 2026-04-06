use super::*;
use crate::discovery_detail_access::collections::load_persisted_collection_series_ids;
use crate::discovery_detail_access::readlists::load_persisted_readlist_book_rows;
use axum_extra::extract::Multipart;

const MOSAIC_HEIGHT: u32 = 300;
const MOSAIC_RATIO: f32 = 0.70666664;

fn thumbnail_dimensions(bytes: &[u8]) -> Option<(i64, i64)> {
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

fn encode_image_bytes_as_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
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

fn response_from_thumbnail_bytes(
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

fn response_from_thumbnail_jpeg_bytes(headers: &HeaderMap, bytes: Vec<u8>) -> Response {
    let Some(jpeg_bytes) = encode_image_bytes_as_jpeg(&bytes) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    response_from_thumbnail_bytes(headers, jpeg_bytes, "image/jpeg")
}

fn response_from_thumbnail_small_jpeg_bytes(
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

fn set_one_hour_private_cache_control(response: &mut Response) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("max-age=3600, private"),
    );
}

fn thumbnail_max_edge_from_setting(value: &str) -> u32 {
    match value {
        "MEDIUM" => 600,
        "LARGE" => 900,
        "XLARGE" => 1200,
        _ => 300,
    }
}

async fn load_book_thumbnail_source_bytes(
    database_file: &FsPath,
    book_id: &str,
    media: &PersistedBookMedia,
) -> Option<Vec<u8>> {
    if let Ok(Some(thumbnail)) = load_selected_book_thumbnail(database_file, book_id).await {
        if thumbnail.thumbnail_type != "GENERATED" {
            return Some(thumbnail.thumbnail);
        }
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

async fn load_series_thumbnail_source_bytes(
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

async fn load_readlist_mosaic_bytes(
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

async fn load_collection_mosaic_bytes(
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

pub async fn book_thumbnail(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        match load_selected_book_thumbnail(auth_db.database_file.as_path(), &book_id).await {
            Ok(Some(thumbnail)) => {
                let etag = asset_etag(thumbnail.thumbnail.as_slice());
                if if_none_match_matches(&headers, etag.as_str()) {
                    return asset_not_modified_response(Some(etag.as_str()), None);
                }

                return asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    Some(etag.as_str()),
                    None,
                );
            }
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn book_thumbnail_opds_response(
    auth_db: &AuthDatabaseState,
    headers: &HeaderMap,
    book_id: &str,
) -> Response {
    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), book_id).await
    {
        if let Some(user) = resolved_auth_user(headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        if let Some(bytes) =
            load_book_thumbnail_source_bytes(auth_db.database_file.as_path(), book_id, &media).await
        {
            return response_from_thumbnail_jpeg_bytes(headers, bytes);
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn book_thumbnail_opds_small_response(
    auth_db: &AuthDatabaseState,
    headers: &HeaderMap,
    book_id: &str,
    max_edge: u32,
) -> Response {
    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), book_id).await
    {
        if let Some(user) = resolved_auth_user(headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        match load_selected_book_thumbnail(auth_db.database_file.as_path(), book_id).await {
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

pub async fn book_thumbnail_opds(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    book_thumbnail_opds_response(&auth_db, &headers, &book_id).await
}

pub async fn book_thumbnail_opds_small(
    Extension(operational): Extension<OperationalState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let settings = match crate::operational_settings_access::server_settings::load_server_settings(
        operational.settings_store.as_ref(),
    )
    .await
    {
        Ok(settings) => settings,
        Err(error) => return internal_error_response(error),
    };

    book_thumbnail_opds_small_response(
        &auth_db,
        &headers,
        &book_id,
        thumbnail_max_edge_from_setting(settings.thumbnail_size),
    )
    .await
}

pub async fn book_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    match load_book_thumbnail_by_id(auth_db.database_file.as_path(), &thumbnail_id).await {
        Ok(Some(thumbnail)) => response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
        && let Some(user) = resolved_auth_user(&headers)
        && !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media)
            .await
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    match load_persisted_book_thumbnails(auth_db.database_file.as_path(), &book_id).await {
        Ok(rows) => {
            if rows.is_empty() {
                if persisted_book_exists(auth_db.database_file.as_path(), &book_id)
                    .await
                    .unwrap_or(false)
                {
                    return Json(json!([])).into_response();
                }

                return StatusCode::NOT_FOUND.into_response();
            }

            let mut response = Json(
                rows.into_iter()
                    .map(|row| {
                        json!({
                            "id": row.id,
                            "bookId": row.book_id,
                            "type": row.thumbnail_type,
                            "selected": row.selected,
                            "mediaType": row.media_type,
                            "fileSize": row.file_size,
                            "width": row.width,
                            "height": row.height,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_book_exists(auth_db.database_file.as_path(), &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "book").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_book_thumbnail(
        auth_db.database_file.as_path(),
        &book_id,
        &thumbnail_bytes,
        media_type.as_str(),
        width,
        height,
        selected,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "bookId": thumbnail.book_id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
            "mediaType": thumbnail.media_type,
            "fileSize": thumbnail.file_size,
            "width": thumbnail.width,
            "height": thumbnail.height,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((_book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_book_thumbnail(auth_db.database_file.as_path(), &thumbnail_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_book_thumbnail(auth_db.database_file.as_path(), &book_id, &thumbnail_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) if error == "only uploaded thumbnails can be deleted" => {
            StatusCode::BAD_REQUEST.into_response()
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match user_can_access_readlist_media(auth_db.database_file.as_path(), &readlist_id, &user).await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                let mut response =
                    response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail.clone());
                set_one_hour_private_cache_control(&mut response);
                return response;
            }

            match load_readlist_mosaic_bytes(auth_db.database_file.as_path(), &readlist_id).await {
                Ok(Some(bytes)) => {
                    let mut response = response_from_thumbnail_bytes(&headers, bytes, "image/jpeg");
                    set_one_hour_private_cache_control(&mut response);
                    return response;
                }
                Ok(None) => {}
                Err(error) => return internal_error_response(error),
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match user_can_access_readlist_media(auth_db.database_file.as_path(), &readlist_id, &user).await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if !rows.is_empty() {
                return Json(
                    rows.into_iter()
                        .map(|row| {
                            json!({
                                "id": row.id,
                                "readListId": row.readlist_id,
                                "type": row.thumbnail_type,
                                "selected": row.selected,
                                "mediaType": row.media_type,
                                "fileSize": row.file_size,
                                "width": row.width,
                                "height": row.height,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
                .into_response();
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return Json(json!([])).into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match user_can_access_readlist_media(auth_db.database_file.as_path(), &readlist_id, &user).await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                return asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    None,
                    None,
                );
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "readlist").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_readlist_thumbnail(
        auth_db.database_file.as_path(),
        &readlist_id,
        &thumbnail_bytes,
        media_type.as_str(),
        width,
        height,
        selected,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "readListId": thumbnail.readlist_id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
            "mediaType": thumbnail.media_type,
            "fileSize": thumbnail.file_size,
            "width": thumbnail.width,
            "height": thumbnail.height,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_readlist_thumbnail(auth_db.database_file.as_path(), &readlist_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_readlist_thumbnail(auth_db.database_file.as_path(), &readlist_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match user_can_access_collection_media(auth_db.database_file.as_path(), &collection_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                let mut response =
                    response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail.clone());
                set_one_hour_private_cache_control(&mut response);
                return response;
            }

            match load_collection_mosaic_bytes(auth_db.database_file.as_path(), &collection_id)
                .await
            {
                Ok(Some(bytes)) => {
                    let mut response = response_from_thumbnail_bytes(&headers, bytes, "image/jpeg");
                    set_one_hour_private_cache_control(&mut response);
                    response
                }
                Ok(None) => StatusCode::NOT_FOUND.into_response(),
                Err(error) => internal_error_response(error),
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match user_can_access_collection_media(auth_db.database_file.as_path(), &collection_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "collectionId": row.collection_id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                        "mediaType": row.media_type,
                        "fileSize": row.file_size,
                        "width": row.width,
                        "height": row.height,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match user_can_access_collection_media(auth_db.database_file.as_path(), &collection_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    None,
                    None,
                )
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "collection").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_bytes,
        media_type.as_str(),
        width,
        height,
        selected,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "collectionId": thumbnail.collection_id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
            "mediaType": thumbnail.media_type,
            "fileSize": thumbnail.file_size,
            "width": thumbnail.width,
            "height": thumbnail.height,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match select_collection_thumbnail(auth_db.database_file.as_path(), &thumbnail_id).await {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;

    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match user_can_access_series_media(auth_db.database_file.as_path(), &resolved_series_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_selected_series_thumbnail(auth_db.database_file.as_path(), &resolved_series_id).await
    {
        Ok(Some(thumbnail)) => {
            return response_from_thumbnail_bytes(&headers, thumbnail.thumbnail, "image/jpeg");
        }
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
    }

    if let Ok(Some(media)) =
        load_persisted_series_thumbnail_media(auth_db.database_file.as_path(), &resolved_series_id)
            .await
        && let Some(bytes) = read_media_file_bytes(&media.file_path)
        && let Some(jpeg_bytes) = encode_image_bytes_as_jpeg(&bytes)
    {
        let etag = asset_etag(jpeg_bytes.as_slice());
        let last_modified = file_last_modified_header_value(media.file_path.as_path());
        if if_none_match_matches(&headers, etag.as_str()) {
            return asset_not_modified_response(Some(etag.as_str()), last_modified.as_deref());
        }

        return asset_ok_response(
            "image/jpeg",
            jpeg_bytes,
            Some(etag.as_str()),
            last_modified.as_deref(),
        );
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn series_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    match user_can_access_series_media(auth_db.database_file.as_path(), &resolved_series_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_persisted_series_thumbnails(auth_db.database_file.as_path(), &resolved_series_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "seriesId": row.series_id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                        "mediaType": row.media_type,
                        "fileSize": row.file_size,
                        "width": row.width,
                        "height": row.height,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if !unrestricted_all_libraries {
        if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }

        match user_can_access_series_media(
            auth_db.database_file.as_path(),
            &resolved_series_id,
            &user,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    match load_series_thumbnail_by_id(auth_db.database_file.as_path(), &thumbnail_id).await {
        Ok(Some(thumbnail)) => asset_ok_response("image/jpeg", thumbnail.thumbnail, None, None),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    match load_persisted_series_oneshot(auth_db.database_file.as_path(), &resolved_series_id).await
    {
        Ok(Some(true)) => return StatusCode::BAD_REQUEST.into_response(),
        Ok(Some(false)) => {}
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "series").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_bytes,
        media_type.as_str(),
        width,
        height,
        selected,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "seriesId": thumbnail.series_id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
            "mediaType": thumbnail.media_type,
            "fileSize": thumbnail.file_size,
            "width": thumbnail.width,
            "height": thumbnail.height,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    match select_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

async fn parse_thumbnail_upload(
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

pub async fn series_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let thumbnail = match load_persisted_series_thumbnails(
        auth_db.database_file.as_path(),
        &resolved_series_id,
    )
    .await
    {
        Ok(rows) => rows.into_iter().find(|row| row.id == thumbnail_id),
        Err(error) => return internal_error_response(error),
    };
    let Some(thumbnail) = thumbnail else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if thumbnail.thumbnail_type != "USER_UPLOADED" {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match delete_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
