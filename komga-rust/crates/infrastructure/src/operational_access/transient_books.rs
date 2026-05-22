use std::path::Path;

use komga_application::operational::{
    TransientBookAnalysis as AppTransientBookAnalysis,
    TransientBookFileMetadata as AppTransientBookFileMetadata,
    TransientBookPage as AppTransientBookPage, TransientBookPort, TransientBookScanEntry,
};

use crate::database_handle::DatabaseHandle;
use crate::filesystem::transient_books::{self, TransientBookPage};

#[derive(Clone)]
pub struct TransientBookAccess {
    db: DatabaseHandle,
}

impl TransientBookAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl TransientBookPort for TransientBookAccess {
    fn analyze_transient_book(&self, path: &str) -> AppTransientBookAnalysis {
        let result = transient_books::analyze_transient_book(path);
        AppTransientBookAnalysis {
            status: result.status,
            media_type: result.media_type,
            page_count: result.page_count,
            pages: result
                .pages
                .into_iter()
                .map(convert_transient_page)
                .collect(),
            files: result.files,
            comment: result.comment,
            number: result.number,
            series_id: result.series_id,
        }
    }

    async fn infer_transient_series_and_number(
        &self,
        transient_name: &str,
    ) -> (Option<String>, Option<f64>) {
        transient_books::infer_transient_series_and_number(self.db.read_pool(), transient_name)
            .await
    }

    fn list_transient_book_entries(&self, root: &Path) -> Vec<TransientBookScanEntry> {
        transient_books::list_transient_book_entries(root)
            .into_iter()
            .filter_map(|entry| {
                Some(TransientBookScanEntry {
                    path: entry.get("path")?.as_str()?.to_string(),
                    name: entry.get("name")?.as_str()?.to_string(),
                })
            })
            .collect()
    }

    async fn validate_transient_scan_root(&self, path: &str) -> Result<(), String> {
        transient_books::validate_transient_scan_root(self.db.read_pool(), Path::new(path)).await
    }

    fn load_transient_book_file_metadata(
        &self,
        path: &str,
    ) -> Option<AppTransientBookFileMetadata> {
        let meta = transient_books::load_transient_book_file_metadata(path)?;
        Some(AppTransientBookFileMetadata {
            file_last_modified_unix_nanos: meta.file_last_modified_unix_nanos,
            size_bytes: meta.size_bytes,
        })
    }

    fn transient_book_exists(&self, path: &str) -> bool {
        transient_books::transient_book_exists(path)
    }

    fn transient_book_page_content(
        &self,
        path: &str,
        media_type: &str,
        pages: &[AppTransientBookPage],
        page_number: u32,
    ) -> Option<(String, Vec<u8>)> {
        let infra_pages: Vec<TransientBookPage> = pages
            .iter()
            .map(|p| TransientBookPage {
                number: p.number,
                file_name: p.file_name.clone(),
                media_type: p.media_type.clone(),
                width: p.width,
                height: p.height,
                size_bytes: p.size_bytes,
            })
            .collect();
        transient_books::transient_book_page_content(path, media_type, &infra_pages, page_number)
    }
}

fn convert_transient_page(page: TransientBookPage) -> AppTransientBookPage {
    AppTransientBookPage {
        number: page.number,
        file_name: page.file_name,
        media_type: page.media_type,
        width: page.width,
        height: page.height,
        size_bytes: page.size_bytes,
    }
}
