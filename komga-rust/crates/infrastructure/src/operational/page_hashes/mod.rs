use komga_application::operational::{
    PageHashDeleteTarget, PageHashKnownEntry, PageHashKnownQuery, PageHashMatchEntry,
    PageHashMatchesQuery, PageHashPage, PageHashPort, PageHashThumbnail, PageHashUnknownEntry,
    PageHashUnknownQuery, PageHashUpsertCommand,
};

use crate::persistence::DatabaseHandle;
use persistence::{
    load_page_hash_delete_targets, load_page_hash_matches_page, load_page_hashes_page,
    load_page_hashes_unknown_page,
};

mod action;
mod mutation;
mod persistence;
mod thumbnails;

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
    ) -> anyhow::Result<PageHashPage<PageHashMatchEntry>> {
        load_page_hash_matches_page(self.db.read_pool(), query)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> anyhow::Result<Option<PageHashThumbnail>> {
        thumbnails::load_page_hash_thumbnail(self.db.read_pool(), page_hash)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> anyhow::Result<Option<PageHashThumbnail>> {
        thumbnails::load_unknown_page_hash_thumbnail(self.db.read_pool(), page_hash, resize_to)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_page_hashes_page(
        &self,
        query: PageHashKnownQuery,
    ) -> anyhow::Result<PageHashPage<PageHashKnownEntry>> {
        load_page_hashes_page(self.db.read_pool(), query)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_page_hashes_unknown_page(
        &self,
        query: PageHashUnknownQuery,
    ) -> anyhow::Result<PageHashPage<PageHashUnknownEntry>> {
        load_page_hashes_unknown_page(self.db.read_pool(), query)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> anyhow::Result<Vec<PageHashDeleteTarget>> {
        load_page_hash_delete_targets(self.db.read_pool(), hash)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn upsert_page_hash(&self, command: PageHashUpsertCommand) -> anyhow::Result<()> {
        thumbnails::upsert_page_hash(self.db.read_pool(), self.db.write_pool(), command)
            .await
            .map_err(anyhow::Error::from)
    }
}
