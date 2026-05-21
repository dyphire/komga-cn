use super::*;
use komga_application::media_assets::MediaReaderPort;

fn decode_epub_extension_positions_and_layout(blob: &[u8]) -> Result<(Vec<Value>, bool), String> {
    let mut decoder = GzDecoder::new(blob);
    let mut json = String::new();
    decoder
        .read_to_string(&mut json)
        .map_err(|error| format!("decode epub extension blob: {error}"))?;
    let payload = serde_json::from_str::<Value>(&json)
        .map_err(|error| format!("parse epub extension blob json: {error}"))?;
    let positions = payload
        .get("positions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let is_fixed_layout = payload
        .get("isFixedLayout")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok((positions, is_fixed_layout))
}

pub(super) fn progression_bad_request(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": message.into() })),
    )
        .into_response()
}

pub(super) fn progression_locator(payload: &Value) -> Option<&Value> {
    payload.get("locator")
}

pub(super) fn locator_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

pub(super) fn locator_position(locator: &Value) -> Option<u64> {
    locator
        .get("locations")
        .and_then(|value| value.get("position"))
        .and_then(Value::as_u64)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = bytes[index + 1] as char;
            let lo = bytes[index + 2] as char;
            let parsed = hi
                .to_digit(16)
                .and_then(|hi| lo.to_digit(16).map(|lo| ((hi << 4) | lo) as u8));
            if let Some(byte) = parsed {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

fn normalized_href_base(href: &str) -> String {
    let base = href.split('#').next().unwrap_or(href).trim_end_matches('#');
    percent_decode(base).trim_start_matches('/').to_string()
}

fn position_progression(position: &Value) -> Option<f64> {
    position
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

fn position_number(position: &Value) -> Option<i64> {
    position
        .get("locations")
        .and_then(|value| value.get("position"))
        .and_then(Value::as_i64)
}

fn position_matches_href(position: &Value, href_base: &str) -> bool {
    position
        .get("href")
        .and_then(Value::as_str)
        .map(|value| normalized_href_base(value) == href_base)
        .unwrap_or(false)
}

fn matched_epub_position(
    positions: &[Value],
    href_base: &str,
    locator_progression: f64,
    is_fixed_layout: bool,
) -> Option<Value> {
    let matching_positions = positions
        .iter()
        .filter(|position| position_matches_href(position, href_base))
        .cloned()
        .collect::<Vec<_>>();

    matching_positions
        .iter()
        .find(|position| position_progression(position) == Some(locator_progression))
        .cloned()
        .or_else(|| {
            if is_fixed_layout && matching_positions.len() == 1 {
                return matching_positions.first().cloned();
            }

            let before = matching_positions
                .iter()
                .filter(|position| {
                    position_progression(position).is_some_and(|value| value < locator_progression)
                })
                .max_by_key(|position| position_number(position))
                .cloned();
            let after = matching_positions
                .iter()
                .filter(|position| {
                    position_progression(position).is_some_and(|value| value > locator_progression)
                })
                .min_by_key(|position| position_number(position))
                .cloned();

            match (before, after) {
                (Some(before), Some(_)) => Some(before),
                _ => None,
            }
        })
}

fn normalized_epub_locator(locator: &Value, matched_position: &Value) -> Value {
    let mut locator = locator.clone();
    let Some(locator_map) = locator.as_object_mut() else {
        return locator;
    };

    locator_map.insert(
        "type".to_string(),
        matched_position
            .get("type")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new())),
    );

    let current_kobo_span_missing = locator_map.get("koboSpan").is_none_or(Value::is_null);
    if current_kobo_span_missing && let Some(kobo_span) = matched_position.get("koboSpan").cloned()
    {
        locator_map.insert("koboSpan".to_string(), kobo_span);
    }

    if let Some(locations) = locator_map
        .get_mut("locations")
        .and_then(Value::as_object_mut)
        && let Some(total_progression) = matched_position
            .get("locations")
            .and_then(|value| value.get("totalProgression"))
            .cloned()
    {
        locations.insert("totalProgression".to_string(), total_progression);
    }

    locator
}

pub(crate) async fn normalize_book_epub_locator(
    reader: &dyn MediaReaderPort,
    book_id: &str,
    locator: &Value,
) -> Result<Value, Response> {
    let href_base = normalized_href_base(
        locator
            .get("href")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if href_base.is_empty() {
        return Err(progression_bad_request("Resource does not exist in book: "));
    }

    let Some(locator_progression) = locator_progression(locator) else {
        return Err(progression_bad_request("location.progression is required"));
    };

    let persisted_media_files = match reader.book_media_files(book_id).await {
        Ok(files) => files,
        Err(error) => return Err(internal_error_response(error)),
    };
    let persisted_resource_exists = (!persisted_media_files.is_empty()).then(|| {
        persisted_media_files
            .iter()
            .any(|file_name| normalized_href_base(file_name) == href_base)
    });
    if persisted_resource_exists == Some(false) {
        return Err(progression_bad_request(format!(
            "Resource does not exist in book: {href_base}"
        )));
    }

    let extension = match reader.epub_extension_blob(book_id).await {
        Ok(extension) => extension,
        Err(error) => return Err(internal_error_response(error)),
    };
    let Some((_class, blob)) = extension else {
        return Err(progression_bad_request("Epub extension not found"));
    };
    let (positions, is_fixed_layout) = match decode_epub_extension_positions_and_layout(&blob) {
        Ok(decoded) => decoded,
        Err(error) => return Err(internal_error_response(error)),
    };

    if persisted_resource_exists.is_none()
        && !positions
            .iter()
            .any(|position| position_matches_href(position, href_base.as_str()))
    {
        return Err(progression_bad_request(format!(
            "Resource does not exist in book: {href_base}"
        )));
    }

    let Some(matched_position) = matched_epub_position(
        &positions,
        href_base.as_str(),
        locator_progression,
        is_fixed_layout,
    ) else {
        return Err(progression_bad_request("Invalid progression"));
    };

    Ok(normalized_epub_locator(locator, &matched_position))
}

pub(crate) async fn progression_is_older_than_existing(
    reader: &dyn MediaReaderPort,
    book_id: &str,
    user_id: &str,
    modified: &str,
) -> Result<bool, String> {
    let Ok(new_modified) = OffsetDateTime::parse(modified, &Rfc3339) else {
        return Ok(false);
    };
    let Some(existing_progression) = reader.book_progression(book_id, user_id).await? else {
        return Ok(false);
    };
    let Some(existing_modified) = existing_progression.get("modified").and_then(Value::as_str)
    else {
        return Ok(false);
    };
    let Ok(existing_modified) = OffsetDateTime::parse(existing_modified, &Rfc3339) else {
        return Ok(false);
    };

    Ok(new_modified <= existing_modified)
}

pub(super) async fn load_epub_locator_for_page(
    app: &MediaAssetsState,
    book_id: &str,
    page: u64,
) -> Result<Option<Value>, String> {
    match app.reader.epub_extension_blob(book_id).await {
        Ok(Some((_class, blob))) => Ok(app
            .content
            .decode_epub_positions_blob(&blob)
            .ok()
            .and_then(|positions| positions.get(page.saturating_sub(1) as usize).cloned())),
        Ok(None) => Ok(None),
        Err(error) => Err(error),
    }
}
