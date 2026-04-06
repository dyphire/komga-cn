use super::*;

pub(super) fn internal_error_response(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

pub(crate) fn attachment_disposition(file_name: &str) -> String {
    format!("attachment; filename=\"=?UTF-8?Q?{file_name}?=\"; filename*=UTF-8''{file_name}",)
}

pub(super) fn inline_disposition(file_name: &str) -> String {
    format!("inline; filename=\"=?UTF-8?Q?{file_name}?=\"; filename*=UTF-8''{file_name}",)
}

pub(super) fn format_size_bytes(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }

    let mut size = size_bytes as f64;
    let mut unit_index = 0usize;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if (size - size.round()).abs() < 0.05 {
        format!("{} {}", size.round() as u64, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}

pub(super) fn requested_byte_range(
    headers: &HeaderMap,
    total_len: usize,
) -> Option<(usize, usize)> {
    let range = headers.get(header::RANGE)?.to_str().ok()?;
    let bytes_range = range.strip_prefix("bytes=")?;
    let (start, end) = bytes_range.split_once('-')?;

    if start.is_empty() {
        return None;
    }

    let start = start.parse::<usize>().ok()?;
    let end = if end.is_empty() {
        total_len.checked_sub(1)?
    } else {
        end.parse::<usize>().ok()?
    };

    if start > end || end >= total_len {
        return None;
    }

    Some((start, end))
}
