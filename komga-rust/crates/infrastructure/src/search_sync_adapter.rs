use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::media_assets::metadata_writer::SearchSyncPort;
use sqlx::SqlitePool;

use crate::search::index_lifecycle::SearchEntityType;
use crate::search::sync::sync_entity_upsert_from_database;

#[derive(Clone)]
pub struct SearchSyncAdapter {
    pool: SqlitePool,
    index_dir: PathBuf,
}

impl SearchSyncAdapter {
    pub fn new(pool: SqlitePool, index_dir: PathBuf) -> Self {
        Self { pool, index_dir }
    }
}

#[async_trait]
impl SearchSyncPort for SearchSyncAdapter {
    async fn sync_book(&self, book_id: &str) -> Result<(), String> {
        sync_entity_upsert_from_database(
            &self.pool,
            &self.index_dir,
            SearchEntityType::Book,
            book_id,
        )
        .await
        .map(|_| ())
    }
}
