use std::path::Path;

use komga_application::operational::{FilesystemBrowsePort, FontPort};
use serde_json::Value;

use crate::filesystem::{browser, fonts};

#[derive(Clone, Default)]
pub struct FilesystemBrowseAccess;

impl FilesystemBrowsePort for FilesystemBrowseAccess {
    fn list_directory_entries(&self, path: &Path, directories_only: bool) -> Vec<Value> {
        browser::list_directory_entries(path, directories_only)
    }
}

#[derive(Clone, Default)]
pub struct FontAccess;

impl FontPort for FontAccess {
    fn list_font_families(&self, path: &Path) -> Vec<String> {
        fonts::list_font_families(path)
    }

    fn load_font_family_css(&self, path: &Path, family: &str) -> Option<String> {
        fonts::load_font_family_css(path, family)
    }

    fn load_font_file(&self, path: &Path, family: &str, file: &str) -> Option<Vec<u8>> {
        fonts::load_font_file(path, family, file)
    }
}
