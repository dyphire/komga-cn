use std::fs;
use std::path::Path;

use serde_json::{Value, json};

pub fn list_directory_entries(path: &Path, directories_only: bool) -> Vec<Value> {
    let mut entries = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|items| items.filter_map(Result::ok))
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let is_directory = file_type.is_dir();
            if directories_only != is_directory {
                return None;
            }

            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_type = if is_directory { "directory" } else { "file" };

            Some(json!({
                "name": name,
                "path": entry_path.to_string_lossy().to_string(),
                "type": entry_type,
            }))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["name"].as_str().unwrap_or_default())
    });
    entries
}
