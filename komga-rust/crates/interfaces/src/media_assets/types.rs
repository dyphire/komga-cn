use super::*;

pub(crate) type PersistedBookMedia = komga_application::media_assets::BookMediaRecord;

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
