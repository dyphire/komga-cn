use komga_domain::discovery::MediaStatus;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransientBookScanEntry {
    pub path: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransientBookSeriesInference {
    pub series_id: Option<String>,
    pub number: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct TransientBookAnalysis {
    pub status: MediaStatus,
    pub media_type: String,
    pub page_count: u32,
    pub pages: Vec<TransientBookPage>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransientBookPageContent {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[async_trait::async_trait]
pub trait TransientBookPort: Send + Sync {
    fn analyze_transient_book(&self, path: &str) -> anyhow::Result<TransientBookAnalysis>;
    async fn infer_transient_series_and_number(
        &self,
        transient_name: &str,
    ) -> anyhow::Result<TransientBookSeriesInference>;
    fn list_transient_book_entries(
        &self,
        root: &Path,
    ) -> anyhow::Result<Vec<TransientBookScanEntry>>;
    async fn validate_transient_scan_root(&self, path: &str) -> anyhow::Result<()>;
    fn load_transient_book_file_metadata(
        &self,
        path: &str,
    ) -> anyhow::Result<TransientBookFileMetadata>;
    fn transient_book_exists(&self, path: &str) -> anyhow::Result<bool>;
    fn transient_book_page_content(
        &self,
        path: &str,
        media_type: &str,
        pages: &[TransientBookPage],
        page_number: u32,
    ) -> anyhow::Result<Option<TransientBookPageContent>>;
}
