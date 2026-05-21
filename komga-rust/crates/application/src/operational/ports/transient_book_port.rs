use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct TransientBookAnalysis {
    pub status: String,
    pub media_type: String,
    pub page_count: u32,
    pub pages: Vec<TransientBookPage>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

#[async_trait]
pub trait TransientBookPort: Send + Sync {
    fn analyze_transient_book(&self, path: &str) -> TransientBookAnalysis;
    async fn infer_transient_series_and_number(
        &self,
        transient_name: &str,
    ) -> (Option<String>, Option<f64>);
    fn list_transient_book_entries(&self, root: &Path) -> Vec<Value>;
    async fn validate_transient_scan_root(&self, path: &str) -> Result<(), String>;
    fn load_transient_book_file_metadata(&self, path: &str) -> Option<TransientBookFileMetadata>;
    fn load_transient_book_media(&self, path: &str) -> Option<Vec<u8>>;
    fn transient_book_content_type(&self, path: &str, media_type: &str) -> &'static str;
    fn transient_book_page_content(
        &self,
        path: &str,
        media_type: &str,
        pages: &[TransientBookPage],
        page_number: u32,
    ) -> Option<(String, Vec<u8>)>;
}
