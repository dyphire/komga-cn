use super::*;
use crate::tasks::media_queries::load_sidecar_url_for_parent as load_persisted_sidecar_url_for_parent;

pub(in crate::task_queue) fn load_sidecar_url_for_parent(
    runtime: &RuntimeConfig,
    parent_url: &str,
    metadata_only: bool,
) -> Result<Option<String>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    load_persisted_sidecar_url_for_parent(
        runtime.database_file.as_path(),
        parent_url,
        metadata_only,
    )
    .map_err(TaskExecutionError::runtime)
}

pub(in crate::task_queue) fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(in crate::task_queue) fn media_type_from_sidecar_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}
