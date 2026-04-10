use super::*;
pub(super) use crate::http::identity_access::auth::{
    authentication_activity_headers_metadata_with_remote_addr, authentication_activity_write_input,
};

pub(super) async fn record_successful_api_key_authentication_by_token(
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    database_file: &FsPath,
    user: &AuthUser,
    api_key: &str,
) -> Option<()> {
    let api_key_metadata = api_key_metadata_by_token(api_key, database_file).await;
    let (api_key_id, api_key_comment) = api_key_metadata
        .as_ref()
        .map(|(id, comment)| (Some(id.as_str()), Some(comment.as_str())))
        .unwrap_or((None, None));

    persisted_record_successful_authentication_activity(
        database_file,
        user,
        authentication_activity_write_input(
            &authentication_activity_headers_metadata_with_remote_addr(headers, remote_addr),
            "API_KEY",
            api_key_id,
            api_key_comment,
        ),
    )
    .await
}

pub(super) async fn api_key_metadata_by_token(
    api_key: &str,
    database_file: &FsPath,
) -> Option<(String, String)> {
    let mut metadata_headers = HeaderMap::new();
    metadata_headers.insert("x-api-key", HeaderValue::from_str(api_key).ok()?);
    persisted_api_key_metadata(&metadata_headers, database_file)
        .await
        .map(|metadata| (metadata.id, metadata.comment))
}

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

#[cfg(test)]
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
    locator: Value,
) -> Value {
    let source = locator.get("href").and_then(Value::as_str);
    let kobo_span = locator.get("koboSpan").and_then(Value::as_str);
    let mut current_bookmark = json!({
        "LastModified": progress.last_modified,
    });
    let current_bookmark_object = current_bookmark
        .as_object_mut()
        .expect("reading state bookmark should be an object");

    if let Some(total_progression) = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
    {
        current_bookmark_object.insert(
            "ProgressPercent".to_string(),
            json!(total_progression * 100.0),
        );
    }
    if let Some(source_progression) = locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
    {
        current_bookmark_object.insert(
            "ContentSourceProgressPercent".to_string(),
            json!(source_progression * 100.0),
        );
    }

    if source.is_some() || kobo_span.is_some() {
        let mut location = json!({
            "Source": source.unwrap_or_default(),
            "Type": "KoboSpan",
        });
        if let Some(kobo_span) = kobo_span {
            location
                .as_object_mut()
                .expect("reading state location should be an object")
                .insert("Value".to_string(), Value::String(kobo_span.to_string()));
        }
        current_bookmark_object.insert("Location".to_string(), location);
    }

    json!({
        "Created": progress.created,
        "CurrentBookmark": current_bookmark,
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
