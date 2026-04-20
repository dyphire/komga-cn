use serde_json::{Value, json};
use time::OffsetDateTime;
use tsid::create_tsid_256;

use crate::helpers::api_file_path;

use super::{TransientBookPageRecord, TransientBookRecord};

pub(super) fn transient_book_id() -> String {
    create_tsid_256().to_string()
}

pub(super) fn transient_book_payload(record: &TransientBookRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "url": api_file_path(&record.path),
        "fileLastModified": format_local_datetime(record.file_last_modified_unix_nanos),
        "sizeBytes": record.size_bytes,
        "size": format_size_bytes(record.size_bytes),
        "status": record.status,
        "mediaType": record.media_type,
        "pages": record.pages.iter().map(transient_page_payload).collect::<Vec<_>>(),
        "files": record.files,
        "comment": record.comment,
        "number": record.number,
        "seriesId": record.series_id,
    })
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

fn format_local_datetime(unix_nanos: i128) -> String {
    let local_offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let datetime = OffsetDateTime::from_unix_timestamp_nanos(unix_nanos)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .to_offset(local_offset);
    let mut formatted = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        datetime.year(),
        datetime.month() as u8,
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second(),
    );
    if datetime.nanosecond() > 0 {
        let fraction = format!("{:09}", datetime.nanosecond());
        formatted.push('.');
        formatted.push_str(fraction.trim_end_matches('0'));
    }
    formatted
}

fn transient_page_payload(page: &TransientBookPageRecord) -> Value {
    json!({
        "number": page.number,
        "fileName": page.file_name,
        "mediaType": page.media_type,
        "width": page.width,
        "height": page.height,
        "sizeBytes": page.size_bytes,
        "size": page
            .size_bytes
            .map(format_size_bytes)
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::{format_local_datetime, transient_book_id};

    #[test]
    fn transient_book_id_uses_kotlin_compatible_tsid_shape() {
        let id = transient_book_id();

        assert_eq!(id.len(), 13);
        assert!(matches!(id.chars().next(), Some('0'..='9' | 'A'..='F')));
        assert!(
            id.chars()
                .all(|ch| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(ch))
        );
    }

    #[test]
    fn format_local_datetime_preserves_subsecond_precision() {
        let formatted = format_local_datetime(123_456_789_i128);
        assert!(
            formatted.contains(".123456789"),
            "expected Kotlin-style local datetime formatting to keep subsecond precision: {formatted}"
        );
    }
}
