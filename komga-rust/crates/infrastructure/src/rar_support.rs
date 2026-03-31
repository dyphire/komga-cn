use std::path::Path;

use unrar::Archive;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RarEntryRecord {
    pub file_name: String,
    pub unpacked_size: u64,
}

pub(crate) fn list_rar_entries(path: &Path) -> Result<Vec<RarEntryRecord>, String> {
    let archive = Archive::new(path)
        .open_for_listing()
        .map_err(|error| format!("open rar for listing '{}': {error}", path.display()))?;

    let mut entries = Vec::new();
    for entry in archive {
        let entry =
            entry.map_err(|error| format!("read rar entry '{}': {error}", path.display()))?;
        if entry.is_directory() {
            continue;
        }
        entries.push(RarEntryRecord {
            file_name: entry.filename.to_string_lossy().replace('\\', "/"),
            unpacked_size: entry.unpacked_size,
        });
    }

    Ok(entries)
}

pub(crate) fn read_rar_entry_bytes(
    path: &Path,
    entry_name: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut archive = Archive::new(path)
        .open_for_processing()
        .map_err(|error| format!("open rar for processing '{}': {error}", path.display()))?;

    while let Some(header) = archive
        .read_header()
        .map_err(|error| format!("read rar header '{}': {error}", path.display()))?
    {
        let current_name = header.entry().filename.to_string_lossy().replace('\\', "/");
        if current_name == entry_name {
            let (data, _rest) = header.read().map_err(|error| {
                format!(
                    "read rar entry '{}' from '{}': {error}",
                    entry_name,
                    path.display()
                )
            })?;
            return Ok(Some(data));
        }
        archive = header.skip().map_err(|error| {
            format!(
                "skip rar entry '{}' in '{}': {error}",
                current_name,
                path.display()
            )
        })?;
    }

    Ok(None)
}
