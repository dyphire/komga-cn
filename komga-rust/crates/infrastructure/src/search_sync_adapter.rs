use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::media_assets::metadata_writer::SearchSyncPort;
use sqlx::SqlitePool;

use crate::search::engine::SearchIndexEngine;

#[derive(Clone)]
pub struct SearchSyncAdapter {
    search: SearchIndexEngine,
}

impl SearchSyncAdapter {
    pub fn new(pool: SqlitePool, index_dir: PathBuf, owns_search_index: bool) -> Self {
        Self {
            search: SearchIndexEngine::new(pool, index_dir, owns_search_index),
        }
    }
}

#[async_trait]
impl SearchSyncPort for SearchSyncAdapter {
    async fn sync_book(&self, book_id: &str) -> Result<(), String> {
        self.search.upsert_book(book_id).await.map(|_| ())
    }
}
