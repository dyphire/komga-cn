use komga_application::media_assets::{BookProgressionInput, ProgressWriterPort};
use komga_application::runtime_sse::RuntimeSseEventSink;
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::filesystem::media_access::read_progress as media_read_progress;
use crate::metadata;

/// Write operations for read progress (book and series level).
/// SSE events are emitted internally by the underlying free functions.
#[derive(Clone)]
pub struct ProgressWriter {
    pool: SqlitePool,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
}

impl ProgressWriter {
    pub fn new(pool: SqlitePool, runtime_events: Arc<dyn RuntimeSseEventSink>) -> Self {
        Self {
            pool,
            runtime_events,
        }
    }
}

#[async_trait::async_trait]
impl ProgressWriterPort for ProgressWriter {
    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> anyhow::Result<()> {
        metadata::persist_read_progress(
            &self.pool,
            self.runtime_events.as_ref(),
            book_id,
            user_id,
            page,
            completed,
            locator,
        )
        .await
    }

    async fn persist_book_progression(&self, input: BookProgressionInput) -> anyhow::Result<()> {
        metadata::persist_book_progression(&self.pool, self.runtime_events.as_ref(), input).await
    }

    async fn delete_read_progress(&self, book_id: &str, user_id: &str) -> anyhow::Result<()> {
        metadata::delete_persisted_read_progress(
            &self.pool,
            self.runtime_events.as_ref(),
            book_id,
            user_id,
        )
        .await
    }

    async fn refresh_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> anyhow::Result<()> {
        media_read_progress::refresh_series_read_progress_row(&self.pool, series_id, user_id).await
    }

    async fn delete_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> anyhow::Result<()> {
        media_read_progress::delete_series_read_progress_row(&self.pool, series_id, user_id).await
    }
}
