use std::path::Path;

pub trait FontPort: Send + Sync {
    fn list_font_families(&self, path: &Path) -> Vec<String>;
    fn load_font_family_css(&self, path: &Path, family: &str) -> Option<String>;
    fn load_font_file(&self, path: &Path, family: &str, file: &str) -> Option<Vec<u8>>;
}
