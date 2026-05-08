use super::*;

pub(crate) fn internal_error_response(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

pub(crate) fn attachment_disposition(file_name: &str) -> String {
    format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_content_disposition_filename(file_name),
        content_disposition_filename_star(file_name)
    )
}

pub(crate) fn inline_disposition(file_name: &str) -> String {
    format!(
        "inline; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_content_disposition_filename(file_name),
        content_disposition_filename_star(file_name)
    )
}

fn ascii_content_disposition_filename(file_name: &str) -> String {
    file_name
        .chars()
        .map(|character| match character {
            ' '..='~' if !matches!(character, '"' | '\\' | ';') => character,
            _ => '_',
        })
        .collect()
}

fn content_disposition_filename_star(file_name: &str) -> String {
    file_name
        .as_bytes()
        .iter()
        .flat_map(|byte| match byte {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'.' | b'_' | b'~' => {
                char::from(*byte).to_string().chars().collect::<Vec<_>>()
            }
            _ => format!("%{byte:02X}").chars().collect::<Vec<_>>(),
        })
        .collect()
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
