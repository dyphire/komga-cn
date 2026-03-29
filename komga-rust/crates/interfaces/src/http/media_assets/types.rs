use super::*;

pub(super) type PersistedBookMedia = komga_application::media_assets::BookMediaRecord;
pub(super) type PersistedBookPageRow = komga_application::media_assets::BookPageRecord;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum ImportCopyMode {
    Move,
    Copy,
    Hardlink,
}

#[derive(Serialize, Deserialize)]
pub(super) struct BooksImportPayload {
    pub(super) copy_mode: ImportCopyMode,
    pub(super) books: Vec<BooksImportEntry>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct BooksImportEntry {
    pub(super) source_file: PathBuf,
    pub(super) series_id: String,
    pub(super) destination_name: Option<String>,
    pub(super) upgrade_book_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ManifestProfile {
    Epub,
    Pdf,
    Divina,
}

#[derive(Clone, Copy)]
pub(super) enum ManifestVariant {
    Default,
    Epub,
    Pdf,
    Divina,
}

pub(super) enum ManifestBuildOutcome {
    Found(&'static str, Value),
    NotFound,
    Forbidden,
}
