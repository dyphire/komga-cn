use komga_application::media_assets::{PageHashDeleteTarget, PageHashThumbnail};
use komga_application::operational::{
    PageHashKnownEntry, PageHashKnownQuery, PageHashMatchEntry, PageHashMatchesQuery, PageHashPage,
    PageHashPort, PageHashUnknownEntry, PageHashUnknownQuery, PageHashUpsertCommand,
};

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
        query: PageHashMatchesQuery,
    ) -> Result<PageHashPage<PageHashMatchEntry>, String> {
        page_hashes_access::load_page_hash_matches_page(self.db.read_pool(), query)
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
        query: PageHashKnownQuery,
    ) -> Result<PageHashPage<PageHashKnownEntry>, String> {
        page_hashes_access::load_page_hashes_page(self.db.read_pool(), query)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_page_hashes_unknown_page(
        &self,
        query: PageHashUnknownQuery,
    ) -> Result<PageHashPage<PageHashUnknownEntry>, String> {
        page_hashes_access::load_page_hashes_unknown_page(self.db.read_pool(), query)
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

    async fn upsert_page_hash(&self, command: PageHashUpsertCommand) -> Result<(), String> {
        page_hashes_access::upsert_page_hash(self.db.read_pool(), self.db.write_pool(), command)
            .await
            .map_err(|e| e.to_string())
    }
}
