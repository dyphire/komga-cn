use async_trait::async_trait;
use serde_json::Value;

use super::super::device_records::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedReadProgressRecord,
};
use super::super::kobo_sync::{KoboLibrarySyncRequest, KoboLibrarySyncResponse};

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait DeviceSyncPort: Send + Sync {
    async fn load_book_created_timestamp(&self, book_id: &str) -> Result<Option<String>, String>;

    async fn load_book_last_epub_position_locator(
        &self,
        book_id: &str,
    ) -> Result<Option<Value>, String>;

    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, String>;

    async fn load_kobo_library_sync(
        &self,
        request: KoboLibrarySyncRequest,
    ) -> Result<KoboLibrarySyncResponse, String>;

    async fn load_koreader_book_target(
        &self,
        book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>;

    async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, String>;

    async fn load_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String>;

    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, String>;

    async fn persist_read_progress_with_locator(
        &self,
        book_id: &str,
        user_id: &str,
        page: i64,
        completed: bool,
        device_id: &str,
        device_name: &str,
        timestamp: &str,
        locator: Option<Value>,
    ) -> Result<(), String>;
}
