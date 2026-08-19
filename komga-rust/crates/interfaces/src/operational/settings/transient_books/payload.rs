use komga_application::operational::{TransientBookPage, TransientBookRecord};

use crate::contracts::common::{KotlinLocalDateTime, WireDateTimeError};
use crate::contracts::transient_books::{TransientBookDto, TransientBookPageDto};
use crate::helpers::api_file_path;

pub(super) fn transient_book_dto(
    record: &TransientBookRecord,
) -> Result<TransientBookDto, WireDateTimeError> {
    Ok(TransientBookDto {
        id: record.id.clone(),
        name: record.name.clone(),
        url: api_file_path(&record.path),
        file_last_modified: KotlinLocalDateTime::from_unix_timestamp_nanos(
            record.file_last_modified_unix_nanos,
        )?,
        size_bytes: record.size_bytes,
        size: format_size_bytes(record.size_bytes),
        status: record.status.persisted_name().to_string(),
        media_type: record.media_type.clone(),
        pages: record.pages.iter().map(transient_page_dto).collect(),
        files: record.files.clone(),
        comment: record.comment.clone(),
        number: record.number,
        series_id: record.series_id.clone(),
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

fn transient_page_dto(page: &TransientBookPage) -> TransientBookPageDto {
    TransientBookPageDto {
        number: page.number,
        file_name: page.file_name.clone(),
        media_type: page.media_type.clone(),
        width: page.width,
        height: page.height,
        size_bytes: page.size_bytes,
        size: page.size_bytes.map(format_size_bytes).unwrap_or_default(),
    }
}
