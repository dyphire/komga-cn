use komga_application::media_assets::{
    CollectionThumbnailRecord, EntityThumbnailRecord, ReadlistThumbnailRecord,
    SeriesThumbnailRecord, ThumbnailWriterPort,
};
use komga_application::runtime_sse::RuntimeSseEventSink;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::metadata;

/// Write operations for thumbnails across all entity types (book, series, readlist, collection).
/// Pure DB writes with no orchestration — side effects (SSE events) are emitted by the
/// underlying free functions.
#[derive(Clone)]
pub struct ThumbnailWriter {
    write_pool: SqlitePool,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
}

impl ThumbnailWriter {
    pub fn new(write_pool: SqlitePool, runtime_events: Arc<dyn RuntimeSseEventSink>) -> Self {
        Self {
            write_pool,
            runtime_events,
        }
    }
}

#[async_trait::async_trait]
impl ThumbnailWriterPort for ThumbnailWriter {
    // --- Book ---

    async fn insert_book(
        &self,
        book_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<EntityThumbnailRecord, String> {
        metadata::insert_book_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            book_id,
            thumbnail,
            media_type,
            width,
            height,
            selected,
        )
        .await
    }

    async fn select_book(&self, thumbnail_id: &str) -> Result<bool, String> {
        metadata::select_book_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            thumbnail_id,
        )
        .await
    }

    async fn delete_book(&self, thumbnail_id: &str) -> Result<bool, String> {
        metadata::delete_book_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            thumbnail_id,
        )
        .await
    }

    // --- Series ---

    async fn insert_series(
        &self,
        series_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<SeriesThumbnailRecord, String> {
        metadata::insert_series_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            series_id,
            thumbnail,
            media_type,
            width,
            height,
            selected,
        )
        .await
    }

    async fn select_series(&self, series_id: &str, thumbnail_id: &str) -> Result<bool, String> {
        metadata::select_series_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            series_id,
            thumbnail_id,
        )
        .await
    }

    async fn delete_series(&self, series_id: &str, thumbnail_id: &str) -> Result<bool, String> {
        metadata::delete_series_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            series_id,
            thumbnail_id,
        )
        .await
    }

    // --- Readlist ---

    async fn insert_readlist(
        &self,
        readlist_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<ReadlistThumbnailRecord, String> {
        metadata::insert_readlist_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            readlist_id,
            thumbnail,
            media_type,
            width,
            height,
            selected,
        )
        .await
    }

    async fn select_readlist(&self, readlist_id: &str, thumbnail_id: &str) -> Result<bool, String> {
        metadata::select_readlist_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            readlist_id,
            thumbnail_id,
        )
        .await
    }

    async fn delete_readlist(&self, readlist_id: &str, thumbnail_id: &str) -> Result<bool, String> {
        metadata::delete_readlist_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            readlist_id,
            thumbnail_id,
        )
        .await
    }

    // --- Collection ---

    async fn insert_collection(
        &self,
        collection_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<CollectionThumbnailRecord, String> {
        metadata::insert_collection_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            collection_id,
            thumbnail,
            media_type,
            width,
            height,
            selected,
        )
        .await
    }

    async fn select_collection(&self, thumbnail_id: &str) -> Result<bool, String> {
        metadata::select_collection_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            thumbnail_id,
        )
        .await
    }

    async fn delete_collection(
        &self,
        collection_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        metadata::delete_collection_thumbnail(
            &self.write_pool,
            self.runtime_events.as_ref(),
            collection_id,
            thumbnail_id,
        )
        .await
    }
}
