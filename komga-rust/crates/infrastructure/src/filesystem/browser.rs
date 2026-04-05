use std::fs;
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use serde_json::{Value, json};

pub fn list_directory_entries(path: &Path, directories_only: bool) -> Vec<Value> {
    let mut entries = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|items| items.filter_map(Result::ok))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry_is_hidden(&entry, &name) {
                return None;
            }

            let file_type = entry.file_type().ok()?;
            let is_directory = file_type.is_dir();
            if directories_only != is_directory {
                return None;
            }

            let entry_path = entry.path();
            let entry_type = if is_directory { "directory" } else { "file" };

            Some(json!({
                "name": name,
                "path": entry_path.to_string_lossy().to_string(),
                "type": entry_type,
            }))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .cmp(&right["path"].as_str().unwrap_or_default().to_lowercase())
    });
    entries
}

#[cfg(windows)]
fn entry_is_hidden(entry: &fs::DirEntry, _name: &str) -> bool {
    entry
        .metadata()
        .map(|metadata| metadata.file_attributes() & 0x2 != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn entry_is_hidden(_entry: &fs::DirEntry, name: &str) -> bool {
    name.starts_with('.')
}
