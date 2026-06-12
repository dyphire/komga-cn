use std::path::Path;

pub trait FontPort: Send + Sync {
    fn list_font_families(&self, path: &Path) -> Result<Vec<String>, String>;
    fn load_font_family_css(&self, path: &Path, family: &str) -> Result<Option<String>, String>;
    fn load_font_file(
        &self,
        path: &Path,
        family: &str,
        file: &str,
    ) -> Result<Option<Vec<u8>>, String>;
}
