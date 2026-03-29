use super::*;

pub(super) async fn build_readlist_archive_payload(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Vec<u8>, String> {
    let entries = load_readlist_archive_entries(database_file, readlist_id).await?;
    if entries.is_empty() {
        return Ok(vec![]);
    }

    let mut archive_entries = Vec::new();
    for (file_name, file_path) in entries {
        if let Some(bytes) = read_media_file_bytes(&file_path) {
            archive_entries.push((file_name, bytes));
        }
    }

    if archive_entries.is_empty() {
        return Ok(vec![]);
    }

    build_stored_zip_archive(archive_entries)
}

pub(super) fn build_stored_zip_archive(entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let mut payload = Vec::new();
    let mut central_directory = Vec::new();
    let mut entries_count: usize = 0;

    for (file_name, bytes) in entries {
        let file_name_bytes = file_name.as_bytes();
        let name_len = u16::try_from(file_name_bytes.len())
            .map_err(|_| format!("zip entry name too long: {file_name}"))?;
        let size =
            u32::try_from(bytes.len()).map_err(|_| format!("zip entry too large: {file_name}"))?;
        let local_header_offset = u32::try_from(payload.len())
            .map_err(|_| "zip archive too large for classic zip format".to_string())?;
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

    let central_directory_offset = u32::try_from(payload.len())
        .map_err(|_| "zip archive too large for classic zip format".to_string())?;
    let central_directory_size = u32::try_from(central_directory.len())
        .map_err(|_| "zip central directory too large for classic zip format".to_string())?;
    let entries_count = u16::try_from(entries_count)
        .map_err(|_| "too many zip entries for classic zip format".to_string())?;

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

fn crc32_ieee(bytes: &[u8]) -> u32 {
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

fn push_u16_le(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes())
}

fn push_u32_le(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes())
}
