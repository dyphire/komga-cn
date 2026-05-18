use serde_json::Value;
use sqlx::SqlitePool;

use crate::filesystem::media_access::read_progress as media_read_progress;
use crate::metadata;

/// Write operations for read progress (book and series level).
/// SSE events are emitted internally by the underlying free functions.
#[derive(Clone)]
pub struct ProgressWriter {
    pool: SqlitePool,
}

impl ProgressWriter {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> Result<(), String> {
        metadata::persist_read_progress(&self.pool, book_id, user_id, page, completed, locator)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn persist_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
        progression: f64,
        use_locator_position_for_page: bool,
        modified: Option<String>,
        device_id: Option<String>,
        device_name: Option<String>,
        locator: Option<Value>,
    ) -> Result<(), String> {
        metadata::persist_book_progression(
            &self.pool,
            book_id,
            user_id,
            progression,
            use_locator_position_for_page,
            modified,
            device_id,
            device_name,
            locator,
        )
        .await
    }

    pub async fn delete_read_progress(&self, book_id: &str, user_id: &str) -> Result<(), String> {
        metadata::delete_persisted_read_progress(&self.pool, book_id, user_id).await
    }

    pub async fn persist_readlist_tachiyomi_progress(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
        last_book_read: usize,
    ) -> Result<Option<()>, String> {
        metadata::persist_readlist_tachiyomi_progress(
            &self.pool,
            ordered_book_ids,
            user_id,
            last_book_read,
        )
        .await
    }

    pub async fn refresh_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        media_read_progress::refresh_series_read_progress_row(&self.pool, series_id, user_id).await
    }

    pub async fn delete_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        media_read_progress::delete_series_read_progress_row(&self.pool, series_id, user_id).await
    }
}
