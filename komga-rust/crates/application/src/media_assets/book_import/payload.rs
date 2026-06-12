use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use crate::task_processing::ImportBookCopyMode as ImportCopyMode;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BooksImportEntry {
    pub source_file: PathBuf,
    pub series_id: String,
    pub destination_name: Option<String>,
    pub upgrade_book_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BooksImportPayload {
    pub copy_mode: ImportCopyMode,
    pub books: Vec<BooksImportEntry>,
}
