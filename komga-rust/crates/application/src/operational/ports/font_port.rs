use std::path::Path;

pub trait FontPort: Send + Sync {
    fn list_font_families(&self, path: &Path) -> anyhow::Result<Vec<String>>;
    fn load_font_family_css(&self, path: &Path, family: &str) -> anyhow::Result<Option<String>>;
    fn load_font_file(
        &self,
        path: &Path,
        family: &str,
        file: &str,
    ) -> anyhow::Result<Option<Vec<u8>>>;
}
