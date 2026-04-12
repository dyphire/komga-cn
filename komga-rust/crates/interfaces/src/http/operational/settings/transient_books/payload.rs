use std::fmt::Write as _;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::http::helpers::api_file_path;

use super::{TransientBookPageRecord, TransientBookRecord};

pub(super) fn transient_book_id(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    let mut id = String::with_capacity(26);
    id.push_str("transient-");
    for byte in digest.iter().take(8) {
        write!(&mut id, "{byte:02x}").expect("writing digest to string should not fail");
    }
    id
}

pub(super) fn transient_book_payload(record: &TransientBookRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "url": api_file_path(&record.path),
        "fileLastModified": format_local_datetime(record.file_last_modified_epoch_seconds),
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

fn format_local_datetime(epoch_seconds: i64) -> String {
    let datetime = OffsetDateTime::from_unix_timestamp(epoch_seconds)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        datetime.year(),
        datetime.month() as u8,
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second(),
    )
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
    use super::transient_book_id;

    #[test]
    fn transient_book_id_keeps_legacy_prefix_and_length() {
        assert_eq!(
            transient_book_id("/tmp/Transient Book.cbz"),
            "transient-bb005341c2e74c7d"
        );
    }
}
