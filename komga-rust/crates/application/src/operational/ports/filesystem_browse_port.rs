use std::path::Path;

use serde_json::Value;

pub trait FilesystemBrowsePort: Send + Sync {
    fn list_directory_entries(&self, path: &Path, directories_only: bool) -> Vec<Value>;
}
