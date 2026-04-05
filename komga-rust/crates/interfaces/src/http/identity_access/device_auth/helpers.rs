use super::*;

pub(super) fn parse_koreader_progress_page(
    progress: &str,
    _page_count: u64,
    _default_progress: f64,
) -> Option<u64> {
    progress.parse::<u64>().ok().filter(|value| *value > 0)
}

pub(super) fn parse_koreader_epub_resource_index(progress: &str) -> Option<usize> {
    let normalized = progress.trim().to_ascii_lowercase();

    if let Some(index) =
        parse_koreader_doc_fragment_index(normalized.as_str(), "docfragment[", ']', true)
    {
        return Some(index);
    }

    parse_koreader_doc_fragment_index(normalized.as_str(), "#_doc_fragment_", '_', false)
}

fn parse_koreader_doc_fragment_index(
    progress: &str,
    prefix: &str,
    suffix: char,
    one_based: bool,
) -> Option<usize> {
    let start = progress.find(prefix)? + prefix.len();
    let tail = &progress[start..];
    let end = tail.find(suffix)?;
    let index = tail[..end].parse::<usize>().ok()?;
    if one_based {
        index.checked_sub(1)
    } else {
        Some(index)
    }
}

pub(super) fn content_type_from_filename(file_name: &str, default_mime_type: &str) -> String {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" => "application/vnd.comicbook+zip".to_string(),
        "cbr" => "application/vnd.comicbook-rar".to_string(),
        "pdf" => "application/pdf".to_string(),
        "epub" => "application/epub+zip".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        _ => default_mime_type.to_string(),
    }
}

pub(super) fn parse_locator_payload(locator: Option<&[u8]>) -> Value {
    locator
        .and_then(|blob| serde_json::from_slice::<Value>(blob).ok())
        .unwrap_or_else(|| json!({}))
}

pub(super) fn kobo_empty_reading_state_payload(book_id: &str, created_timestamp: &str) -> Value {
    json!({
        "Created": created_timestamp,
        "CurrentBookmark": {
            "LastModified": created_timestamp,
            "ProgressPercent": 0.0,
            "ContentSourceProgressPercent": 0.0,
            "Location": {
                "Source": Value::Null,
                "Type": "koboSpan",
                "Value": Value::Null,
            }
        },
        "EntitlementId": book_id,
        "LastModified": created_timestamp,
        "PriorityTimestamp": created_timestamp,
        "Statistics": {
            "LastModified": created_timestamp,
        },
        "StatusInfo": {
            "LastModified": created_timestamp,
            "Status": "ReadyToRead",
            "TimesStartedReading": 0,
        },
    })
}

pub(super) fn kobo_reading_state_payload(
    book_id: &str,
    progress: &PersistedReadProgressRecord,
    page_count: u64,
    locator: Value,
) -> Value {
    let total_progression = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            (progress.page.max(0) as f64 / page_count.max(1) as f64).clamp(0.0, 1.0)
        });
    let source_progression = locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
        .unwrap_or(total_progression);
    let source = locator
        .get("href")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kobo_span = locator
        .get("koboSpan")
        .and_then(Value::as_str)
        .unwrap_or_default();

    json!({
        "Created": progress.created,
        "CurrentBookmark": {
            "LastModified": progress.last_modified,
            "ProgressPercent": total_progression * 100.0,
            "ContentSourceProgressPercent": source_progression * 100.0,
            "Location": {
                "Source": if source.is_empty() { Value::Null } else { Value::String(source.to_string()) },
                "Type": "koboSpan",
                "Value": if kobo_span.is_empty() { Value::Null } else { Value::String(kobo_span.to_string()) },
            }
        },
        "EntitlementId": book_id,
        "LastModified": progress.last_modified,
        "PriorityTimestamp": progress.last_modified,
        "Statistics": {
            "LastModified": progress.last_modified,
        },
        "StatusInfo": {
            "LastModified": progress.last_modified,
            "Status": if progress.completed { "Finished" } else { "Reading" },
            "TimesStartedReading": 1,
        },
    })
}
