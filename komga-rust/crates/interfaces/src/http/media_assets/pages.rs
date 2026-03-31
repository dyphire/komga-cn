use super::*;

pub async fn book_page(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_book_id =
        resolve_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

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
        let book_display_name =
            load_persisted_manifest_book(auth_db.database_file.as_path(), &resolved_book_id)
                .await
                .ok()
                .flatten()
                .map(|(_, title, _)| title)
                .unwrap_or_else(|| media.file_name.clone());

        if !book_media_is_ready_status(auth_db.database_file.as_path(), &resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }

        if let Some(user) = resolved_auth_user(&headers) {
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

        if book_media_is_pdf(&media) && content_negotiation && accept_header_prefers_pdf(&headers) {
            if requested_page_number == 0 {
                return StatusCode::BAD_REQUEST.into_response();
            }
            let page_count = detect_pdf_page_count(&media).unwrap_or(media.page_count);
            if requested_page_number as u64 > page_count {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if let Some(bytes) =
                read_pdf_page_as_single_page_pdf(&media, requested_page_number as u64)
            {
                let last_modified = file_last_modified_header_value(media.file_path.as_path());
                if let Some(last_modified) = last_modified.as_deref()
                    && if_modified_since_matches(&headers, last_modified)
                {
                    return asset_not_modified_response(None, Some(last_modified));
                }

                let mut response =
                    asset_ok_response("application/pdf", bytes, None, last_modified.as_deref());
                let file_name = page_response_file_name(
                    &book_display_name,
                    requested_page_number,
                    "application/pdf",
                );
                response.headers_mut().insert(
                    header::CONTENT_DISPOSITION,
                    HeaderValue::from_str(&inline_disposition(&file_name))
                        .expect("page pdf content disposition should be valid"),
                );
                return response;
            }
            return StatusCode::NOT_FOUND.into_response();
        }

        let page_row = match load_persisted_book_page_row(
            auth_db.database_file.as_path(),
            &resolved_book_id,
            requested_page_number as u64,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) if book_media_is_single_image(&media) && requested_page_number == 1 => {
                PersistedBookPageRow {
                    number: requested_page_number as u64,
                    file_name: media.file_name.clone(),
                    media_type: content_type_from_filename(&media.file_name, &media.media_type),
                    width: None,
                    height: None,
                    file_size: read_media_file_size(&media.file_path).unwrap_or(0),
                }
            }
            Ok(None) => {
                if let Some(row) = load_archive_page_row(&media, requested_page_number as u64) {
                    row
                } else if let Some(row) = load_pdf_page_row(&media, requested_page_number as u64) {
                    row
                } else {
                    return StatusCode::NOT_FOUND.into_response();
                }
            }
            Err(error) => return internal_error_response(error),
        };

        if let Some(bytes) =
            resolve_book_page_bytes(&media, &page_row, requested_page_number as u64)
        {
            let mut effective_bytes = bytes;
            let content_type = if page_row.media_type.is_empty() {
                content_type_from_filename(&page_row.file_name, &media.media_type)
            } else {
                page_row.media_type
            };

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
                && if_modified_since_matches(&headers, last_modified)
            {
                return asset_not_modified_response(None, Some(last_modified));
            }

            let mut response = asset_ok_response(
                effective_content_type.as_str(),
                effective_bytes,
                None,
                last_modified.as_deref(),
            );
            let file_name = page_response_file_name(
                &book_display_name,
                requested_page_number,
                effective_content_type.as_str(),
            );
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&inline_disposition(&file_name))
                    .expect("page content disposition should be valid"),
            );
            return response;
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_page_raw(
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
        let book_display_name =
            load_persisted_manifest_book(auth_db.database_file.as_path(), &resolved_book_id)
                .await
                .ok()
                .flatten()
                .map(|(_, title, _)| title)
                .unwrap_or_else(|| media.file_name.clone());

        if !book_media_is_ready_status(auth_db.database_file.as_path(), &resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }

        if !book_media_is_pdf(&media) {
            return StatusCode::BAD_REQUEST.into_response();
        }

        if let Some(user) = resolved_auth_user(&headers) {
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

        let page_count = detect_pdf_page_count(&media).unwrap_or(media.page_count);
        if page_number == 0 || page_number as u64 > page_count {
            return StatusCode::BAD_REQUEST.into_response();
        }

        if let Some(bytes) = read_pdf_page_as_single_page_pdf(&media, page_number as u64) {
            let last_modified = file_last_modified_header_value(media.file_path.as_path());
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(&headers, last_modified)
            {
                return asset_not_modified_response(None, Some(last_modified));
            }

            let mut response =
                asset_ok_response("application/pdf", bytes, None, last_modified.as_deref());
            let file_name =
                page_response_file_name(&book_display_name, page_number, "application/pdf");
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&inline_disposition(&file_name))
                    .expect("raw page content disposition should be valid"),
            );
            return response;
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

        let page_row = match load_persisted_book_page_row(
            auth_db.database_file.as_path(),
            &resolved_book_id,
            page_number as u64,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) if book_media_is_single_image(&media) && page_number == 1 => {
                PersistedBookPageRow {
                    number: page_number as u64,
                    file_name: media.file_name.clone(),
                    media_type: content_type_from_filename(&media.file_name, &media.media_type),
                    width: None,
                    height: None,
                    file_size: read_media_file_size(&media.file_path).unwrap_or(0),
                }
            }
            Ok(None) => {
                if let Some(row) = load_archive_page_row(&media, page_number as u64) {
                    row
                } else {
                    return StatusCode::NOT_FOUND.into_response();
                }
            }
            Err(error) => return internal_error_response(error),
        };

        if let Some(bytes) = resolve_book_page_bytes(&media, &page_row, page_number as u64) {
            let content_type = if page_row.media_type.is_empty() {
                content_type_from_filename(&page_row.file_name, &media.media_type)
            } else {
                page_row.media_type
            };

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
        && !user_can_access_library(&user, &media.library_id)
    {
        return StatusCode::FORBIDDEN.into_response();
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
        return Json(
            page_rows
                .into_iter()
                .map(|page| {
                    json!({
                        "number": page.number,
                        "fileName": page.file_name,
                        "mediaType": page.media_type,
                        "width": page.width,
                        "height": page.height,
                        "sizeBytes": page.file_size,
                        "size": format_size_bytes(page.file_size as u64),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
    }

    if let Some(archive_rows) = load_archive_page_rows(&media)
        && !archive_rows.is_empty()
    {
        return Json(
            archive_rows
                .into_iter()
                .map(|page| {
                    json!({
                        "number": page.number,
                        "fileName": page.file_name,
                        "mediaType": page.media_type,
                        "width": page.width,
                        "height": page.height,
                        "sizeBytes": page.file_size,
                        "size": format_size_bytes(page.file_size as u64),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
    }

    let generated_pdf_rows = load_generated_pdf_page_rows(&media);
    if !generated_pdf_rows.is_empty() {
        return Json(
            generated_pdf_rows
                .into_iter()
                .map(|page| {
                    json!({
                        "number": page.number,
                        "fileName": page.file_name,
                        "mediaType": page.media_type,
                        "width": page.width,
                        "height": page.height,
                        "sizeBytes": page.file_size,
                        "size": format_size_bytes(page.file_size as u64),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
    }

    if !book_media_is_single_image(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let size_bytes = read_media_file_size(&media.file_path).unwrap_or(0).max(0) as u64;

    Json(vec![json!({
        "number": 1,
        "fileName": media.file_name,
        "mediaType": content_type_from_filename(&media.file_name, &media.media_type),
        "width": Value::Null,
        "height": Value::Null,
        "sizeBytes": size_bytes,
        "size": format_size_bytes(size_bytes),
    })])
    .into_response()
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

    if book_media_is_epub(&media) {
        match load_persisted_epub_positions(auth_db.database_file.as_path(), &resolved_book_id)
            .await
        {
            Ok(Some(positions)) if !positions.is_empty() => {
                return Json(json!({
                    "total": positions.len(),
                    "positions": positions,
                }))
                .into_response();
            }
            Ok(_) => {}
            Err(error) => return internal_error_response(error),
        }

        if let Some(positions) = load_epub_archive_positions_from_file(&media)
            && !positions.is_empty()
        {
            return Json(json!({
                "total": positions.len(),
                "positions": positions,
            }))
            .into_response();
        }
    }

    let persisted_page_rows =
        match load_persisted_book_pages(auth_db.database_file.as_path(), &resolved_book_id).await {
            Ok(rows) => rows,
            Err(error) => return internal_error_response(error),
        };

    let effective_page_rows = if persisted_page_rows.is_empty() {
        load_archive_page_rows(&media)
            .filter(|rows| !rows.is_empty())
            .unwrap_or_else(|| load_generated_pdf_page_rows(&media))
    } else {
        persisted_page_rows
    };

    let generated_page_numbers = if effective_page_rows.is_empty() {
        if book_media_is_single_image(&media) {
            vec![1]
        } else if media.page_count > 0 {
            (1..=media.page_count).collect::<Vec<_>>()
        } else {
            return StatusCode::NOT_FOUND.into_response();
        }
    } else {
        effective_page_rows
            .iter()
            .map(|page| page.number)
            .collect::<Vec<_>>()
    };

    let total = generated_page_numbers.len();
    let positions = generated_page_numbers
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let position = index + 1;
            let progression = position as f64 / total as f64;
            let use_manifest_position_link =
                effective_page_rows.is_empty() && !book_media_is_single_image(&media);
            let href = if use_manifest_position_link {
                format!("/api/v1/books/{resolved_book_id}/manifest#position={position}")
            } else {
                format!("/api/v1/books/{resolved_book_id}/pages/{page}")
            };
            let content_type = if use_manifest_position_link {
                "application/webpub+json".to_string()
            } else {
                effective_page_rows
                    .get(index)
                    .map(|row| {
                        if row.media_type.is_empty() {
                            content_type_from_filename(&row.file_name, &media.media_type)
                        } else {
                            row.media_type.clone()
                        }
                    })
                    .unwrap_or_else(|| {
                        content_type_from_filename(&media.file_name, &media.media_type)
                    })
            };

            json!({
                "href": href,
                "type": content_type,
                "title": if effective_page_rows.is_empty() {
                    format!("{}#{page}", media.file_name)
                } else {
                    effective_page_rows[index].file_name.clone()
                },
                "locations": {
                    "position": position,
                    "progression": progression,
                    "totalProgression": progression,
                },
            })
        })
        .collect::<Vec<_>>();

    Json(json!({
        "total": total,
        "positions": positions,
    }))
    .into_response()
}
