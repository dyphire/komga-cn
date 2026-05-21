use komga_application::media_assets::{PageHashDeleteTarget, PageHashThumbnail};
use komga_application::operational::PageHashPort;

use crate::database_handle::DatabaseHandle;
use crate::page_hashes_access;

#[derive(Clone)]
pub struct PageHashAccess {
    db: DatabaseHandle,
}

impl PageHashAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl PageHashPort for PageHashAccess {
    async fn load_page_hash_matches_page(
        &self,
        page_hash: &str,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<serde_json::Value, String> {
        page_hashes_access::load_page_hash_matches_page(
            self.db.read_pool(),
            page_hash,
            page,
            size,
            sorts,
        )
        .await
        .map_err(|e| e.to_string())
    }

    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> Result<Option<PageHashThumbnail>, String> {
        page_hashes_access::load_page_hash_thumbnail(self.db.read_pool(), page_hash)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, String> {
        page_hashes_access::load_unknown_page_hash_thumbnail(
            self.db.read_pool(),
            page_hash,
            resize_to,
        )
        .await
        .map_err(|e| e.to_string())
    }

    async fn load_page_hashes_page(
        &self,
        page: u64,
        size: u64,
        actions: &[String],
        sorts: &[String],
    ) -> Result<serde_json::Value, String> {
        page_hashes_access::load_page_hashes_page(self.db.read_pool(), page, size, actions, sorts)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<serde_json::Value, String> {
        page_hashes_access::load_page_hashes_unknown_page(self.db.read_pool(), page, size, sorts)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> Result<Vec<PageHashDeleteTarget>, String> {
        page_hashes_access::load_page_hash_delete_targets(self.db.read_pool(), hash)
            .await
            .map_err(|e| e.to_string())
    }

    async fn upsert_page_hash(
        &self,
        hash: &str,
        size: Option<i64>,
        action: &str,
    ) -> Result<(), String> {
        page_hashes_access::upsert_page_hash(
            self.db.read_pool(),
            self.db.write_pool(),
            hash,
            size,
            action,
        )
        .await
        .map_err(|e| e.to_string())
    }
}
