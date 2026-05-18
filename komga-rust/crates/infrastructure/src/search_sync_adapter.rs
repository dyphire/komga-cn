use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::media_assets::metadata_writer::SearchSyncPort;
use sqlx::SqlitePool;

use crate::search::index_lifecycle::SearchEntityType;
use crate::search::runtime_tasks;

#[derive(Clone)]
pub struct SearchSyncAdapter {
    pool: SqlitePool,
    database_file: PathBuf,
    index_dir: PathBuf,
}

impl SearchSyncAdapter {
    pub fn new(pool: SqlitePool, database_file: PathBuf, index_dir: PathBuf) -> Self {
        Self {
            pool,
            database_file,
            index_dir,
        }
    }
}

#[async_trait]
impl SearchSyncPort for SearchSyncAdapter {
    async fn sync_book(&self, book_id: &str) -> Result<(), String> {
        runtime_tasks::sync_entity_upsert_from_database(
            &self.pool,
            &self.database_file,
            &self.index_dir,
            SearchEntityType::Book,
            book_id,
        )
        .await
        .map(|_| ())
    }
}
