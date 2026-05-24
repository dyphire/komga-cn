use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::media_assets::{
    BookMediaPort, BookMediaRecord, BookPageRecord, CollectionThumbnailRecord, ContentAccessPort,
    EntityExistencePort, EntityThumbnailBinary, EntityThumbnailRecord, PersistedMediaFileRecord,
    ReadProgressReadPort, ReadlistThumbnailRecord, SeriesRelationPort, SeriesThumbnailRecord,
    ThumbnailReadPort,
};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::filesystem::media_access::db_queries::{self};
use crate::filesystem::media_access::read_progress as media_read_progress;
use crate::metadata;

/// Direct read access to media assets backed by SQLite.
/// No trait indirection — SqlitePool is Arc internally, so this is Clone and cheap.
#[derive(Clone)]
pub struct MediaReader {
    read_pool: SqlitePool,
}

impl MediaReader {
    pub fn new(read_pool: SqlitePool) -> Self {
        Self { read_pool }
    }
}

#[async_trait]
impl BookMediaPort for MediaReader {
    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        db_queries::load_persisted_book_media(&self.read_pool, book_id).await
    }

    async fn book_media_files(&self, book_id: &str) -> Result<Vec<String>, String> {
        db_queries::load_persisted_book_media_files(&self.read_pool, book_id).await
    }

    async fn media_file_records(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String> {
        db_queries::load_persisted_media_file_records(&self.read_pool, book_id)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| PersistedMediaFileRecord {
                        file_name: row.file_name,
                        media_type: row.media_type,
                        sub_type: row.sub_type,
                    })
                    .collect()
            })
    }

    async fn book_media_is_ready(&self, book_id: &str) -> Result<bool, String> {
        db_queries::book_media_is_ready_status(&self.read_pool, book_id).await
    }

    async fn book_pages(&self, book_id: &str) -> Result<Vec<BookPageRecord>, String> {
        db_queries::load_persisted_book_pages(&self.read_pool, book_id).await
    }

    async fn book_page(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String> {
        db_queries::load_persisted_book_page_row(&self.read_pool, book_id, page_number).await
    }

    async fn epub_extension_blob(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        db_queries::load_persisted_epub_extension_blob(&self.read_pool, book_id).await
    }
}

#[async_trait]
impl SeriesRelationPort for MediaReader {
    async fn series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String> {
        db_queries::load_series_book_ids(&self.read_pool, series_id).await
    }

    async fn series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> Result<Vec<(String, f64)>, String> {
        db_queries::load_series_book_number_sorts(&self.read_pool, series_id).await
    }

    async fn series_oneshot(&self, series_id: &str) -> Result<Option<bool>, String> {
        db_queries::load_persisted_series_oneshot(&self.read_pool, series_id).await
    }
}

#[async_trait]
impl EntityExistencePort for MediaReader {
    async fn book_exists(&self, book_id: &str) -> Result<bool, String> {
        db_queries::persisted_book_exists(&self.read_pool, book_id).await
    }

    async fn series_exists(&self, series_id: &str) -> Result<bool, String> {
        db_queries::persisted_series_exists(&self.read_pool, series_id).await
    }

    async fn readlist_exists(&self, readlist_id: &str) -> Result<bool, String> {
        metadata::persisted_readlist_exists(&self.read_pool, readlist_id).await
    }

    async fn collection_exists(&self, collection_id: &str) -> Result<bool, String> {
        metadata::persisted_collection_exists(&self.read_pool, collection_id).await
    }
}

#[async_trait]
impl ContentAccessPort for MediaReader {
    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        db_queries::load_book_restrictions(&self.read_pool, book_id).await
    }

    async fn readlist_archive_entries(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<(String, PathBuf)>, String> {
        db_queries::load_readlist_archive_entries(&self.read_pool, readlist_id).await
    }

    async fn series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
        db_queries::load_series_archive_entries(&self.read_pool, series_id).await
    }

    async fn manifest_book(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, String, String)>, String> {
        db_queries::load_persisted_manifest_book(&self.read_pool, book_id).await
    }

    async fn readlist_name(&self, readlist_id: &str) -> Result<Option<String>, String> {
        metadata::load_persisted_readlist_name(&self.read_pool, readlist_id).await
    }
}

#[async_trait]
impl ThumbnailReadPort for MediaReader {
    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        metadata::load_selected_book_thumbnail(&self.read_pool, book_id).await
    }

    async fn book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        metadata::load_book_thumbnail_by_id(&self.read_pool, thumbnail_id).await
    }

    async fn book_thumbnails(&self, book_id: &str) -> Result<Vec<EntityThumbnailRecord>, String> {
        metadata::load_persisted_book_thumbnails(&self.read_pool, book_id).await
    }

    async fn selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        metadata::load_selected_series_thumbnail(&self.read_pool, series_id).await
    }

    async fn series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        metadata::load_series_thumbnail_by_id(&self.read_pool, thumbnail_id).await
    }

    async fn series_thumbnails(
        &self,
        series_id: &str,
    ) -> Result<Vec<SeriesThumbnailRecord>, String> {
        metadata::load_persisted_series_thumbnails(&self.read_pool, series_id).await
    }

    async fn readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<ReadlistThumbnailRecord>, String> {
        metadata::load_persisted_readlist_thumbnails(&self.read_pool, readlist_id).await
    }

    async fn collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionThumbnailRecord>, String> {
        metadata::load_persisted_collection_thumbnails(&self.read_pool, collection_id).await
    }
}

#[async_trait]
impl ReadProgressReadPort for MediaReader {
    async fn book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String> {
        metadata::load_book_progression(&self.read_pool, book_id, user_id).await
    }

    async fn book_read_progress_completed(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<bool>, String> {
        metadata::load_book_read_progress_completed(&self.read_pool, book_id, user_id).await
    }

    async fn series_tachiyomi_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String> {
        media_read_progress::load_series_tachiyomi_progress(&self.read_pool, series_id, user_id)
            .await
    }

    async fn readlist_tachiyomi_counters(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> Result<(u64, u64, u64, u64, u64), String> {
        metadata::readlist_tachiyomi_counters(&self.read_pool, ordered_book_ids, user_id).await
    }

    async fn book_page_count(&self, book_id: &str) -> Result<Option<u64>, String> {
        metadata::load_book_page_count(&self.read_pool, book_id).await
    }
}
