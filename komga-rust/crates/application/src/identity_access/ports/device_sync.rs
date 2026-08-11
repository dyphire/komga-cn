use super::super::device_records::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedReadProgressRecord,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceThumbnailBinary {
    pub book_id: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[async_trait::async_trait]
pub trait DeviceSyncPort: Send + Sync {
    async fn load_book_created_timestamp(&self, book_id: &str) -> Result<Option<String>, String>;

    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, String>;

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
    ) -> Result<Option<DeviceThumbnailBinary>, String>;

    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, String>;
}
