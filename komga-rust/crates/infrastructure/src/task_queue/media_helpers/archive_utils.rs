use std::fs;
use std::path::Path;
use std::path::PathBuf;

use komga_application::task_processing::TaskProcessingError;

use crate::rar_support::read_rar_entries_bytes;

pub(in crate::task_queue) fn normalize_library_relative_url(
    library_root: &PathBuf,
    absolute_path: &Path,
) -> Result<String, TaskProcessingError> {
    let relative = absolute_path.strip_prefix(library_root).map_err(|error| {
        TaskProcessingError::runtime(format!(
            "failed to derive relative path '{}' from library root '{}': {error}",
            absolute_path.display(),
            library_root.display(),
        ))
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

pub(in crate::task_queue) struct StoredArchiveEntry {
    pub(in crate::task_queue) file_name: String,
    pub(in crate::task_queue) bytes: Vec<u8>,
}

pub(in crate::task_queue) fn load_rar_entries_for_conversion(
    source_path: &Path,
) -> Result<Vec<StoredArchiveEntry>, TaskProcessingError> {
    read_rar_entries_bytes(source_path)
        .map_err(TaskProcessingError::runtime)
        .map(|entries| {
            entries
                .into_iter()
                .map(|entry| StoredArchiveEntry {
                    file_name: entry.file_name,
                    bytes: entry.bytes,
                })
                .collect()
        })
}

pub(in crate::task_queue) fn build_stored_zip_archive(
    entries: Vec<StoredArchiveEntry>,
) -> Result<Vec<u8>, TaskProcessingError> {
    let mut payload = Vec::new();
    let mut central_directory = Vec::new();
    let mut entries_count: usize = 0;

    for StoredArchiveEntry { file_name, bytes } in entries {
        let file_name_bytes = file_name.as_bytes();
        let name_len = u16::try_from(file_name_bytes.len()).map_err(|_| {
            TaskProcessingError::runtime(format!("zip entry name too long: {file_name}"))
        })?;
        let size = u32::try_from(bytes.len()).map_err(|_| {
            TaskProcessingError::runtime(format!("zip entry too large: {file_name}"))
        })?;
        let local_header_offset = u32::try_from(payload.len()).map_err(|_| {
            TaskProcessingError::runtime("zip archive too large for classic zip format")
        })?;
        let crc32 = crc32_ieee(&bytes);

        push_u32_le(&mut payload, 0x0403_4b50);
        push_u16_le(&mut payload, 20);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u32_le(&mut payload, crc32);
        push_u32_le(&mut payload, size);
        push_u32_le(&mut payload, size);
        push_u16_le(&mut payload, name_len);
        push_u16_le(&mut payload, 0);
        payload.extend_from_slice(file_name_bytes);
        payload.extend_from_slice(&bytes);

        push_u32_le(&mut central_directory, 0x0201_4b50);
        push_u16_le(&mut central_directory, 20);
        push_u16_le(&mut central_directory, 20);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, crc32);
        push_u32_le(&mut central_directory, size);
        push_u32_le(&mut central_directory, size);
        push_u16_le(&mut central_directory, name_len);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, local_header_offset);
        central_directory.extend_from_slice(file_name_bytes);
        entries_count += 1;
    }

    let central_directory_offset = u32::try_from(payload.len()).map_err(|_| {
        TaskProcessingError::runtime("zip archive too large for classic zip format")
    })?;
    let central_directory_size = u32::try_from(central_directory.len()).map_err(|_| {
        TaskProcessingError::runtime("zip central directory too large for classic zip format")
    })?;
    let entries_count = u16::try_from(entries_count)
        .map_err(|_| TaskProcessingError::runtime("too many zip entries for classic zip format"))?;

    payload.extend_from_slice(&central_directory);
    push_u32_le(&mut payload, 0x0605_4b50);
    push_u16_le(&mut payload, 0);
    push_u16_le(&mut payload, 0);
    push_u16_le(&mut payload, entries_count);
    push_u16_le(&mut payload, entries_count);
    push_u32_le(&mut payload, central_directory_size);
    push_u32_le(&mut payload, central_directory_offset);
    push_u16_le(&mut payload, 0);

    Ok(payload)
}

pub(in crate::task_queue) fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let lsb = crc & 1;
            crc >>= 1;
            if lsb != 0 {
                crc ^= 0xedb8_8320;
            }
        }
    }
    !crc
}

pub(in crate::task_queue) fn push_u16_le(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

pub(in crate::task_queue) fn push_u32_le(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn to_unix_seconds(time: std::time::SystemTime, path: &Path) -> Result<i64, String> {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).map_err(|_| {
            format!(
                "filesystem timestamp for '{}' is outside i64 range",
                path.display()
            )
        }),
        Err(error) => {
            let duration = error.duration();
            let seconds = i64::try_from(duration.as_secs()).map_err(|_| {
                format!(
                    "filesystem timestamp for '{}' is outside i64 range",
                    path.display()
                )
            })?;
            Ok(-seconds)
        }
    }
}

pub(in crate::task_queue) fn metadata_updated_unix_seconds(
    metadata: &fs::Metadata,
    path: &Path,
) -> Result<i64, String> {
    [metadata.created().ok(), metadata.modified().ok()]
        .into_iter()
        .flatten()
        .map(|time| to_unix_seconds(time, path))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or_else(|| {
            format!(
                "failed to read created or modified timestamp for '{}'",
                path.display()
            )
        })
}
