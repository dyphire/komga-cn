#![allow(unused_variables)]

use super::*;
use async_trait::async_trait;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::runtime_identity_access::IdentityAccess;
use komga_infrastructure::sqlite::setup;
use std::path::PathBuf;

pub(crate) async fn test_identity_state() -> IdentityState {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should connect");
    setup::bootstrap_pool(&pool)
        .await
        .expect("test sqlite pool should bootstrap");
    let handle = DatabaseHandle::single_pool(PathBuf::from(":memory:"), pool);
    IdentityState::new(Arc::new(IdentityAccess::new(handle)))
}

pub(crate) struct NoopBookMetadataPort;
#[async_trait]
impl komga_application::media_assets::BookMetadataPort for NoopBookMetadataPort {
    async fn load_book_metadata(
        &self,
        _book_id: &str,
    ) -> Result<Option<komga_application::media_assets::BookMetadata>, String> {
        Ok(None)
    }
    async fn load_book_series_id(&self, _book_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    async fn load_book_library_id(&self, _book_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    async fn persist_book_metadata(
        &self,
        _book_id: &str,
        _metadata: &komga_application::media_assets::BookMetadata,
    ) -> Result<bool, String> {
        Ok(false)
    }
}

pub(crate) struct NoopSearchSyncPort;
#[async_trait]
impl komga_application::media_assets::metadata_writer::SearchSyncPort for NoopSearchSyncPort {
    async fn sync_book(&self, _book_id: &str) -> Result<(), String> {
        Ok(())
    }
}
