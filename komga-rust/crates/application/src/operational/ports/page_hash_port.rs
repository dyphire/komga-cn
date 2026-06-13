use crate::operational::{
    PageHashDeleteTarget, PageHashKnownEntry, PageHashKnownQuery, PageHashMatchEntry,
    PageHashMatchesQuery, PageHashPage, PageHashThumbnail, PageHashUnknownEntry,
    PageHashUnknownQuery, PageHashUpsertCommand,
};

#[async_trait::async_trait]
pub trait PageHashPort: Send + Sync {
    async fn load_page_hash_matches_page(
        &self,
        query: PageHashMatchesQuery,
    ) -> Result<PageHashPage<PageHashMatchEntry>, String>;
    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> Result<Option<PageHashThumbnail>, String>;
    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, String>;
    async fn load_page_hashes_page(
        &self,
        query: PageHashKnownQuery,
    ) -> Result<PageHashPage<PageHashKnownEntry>, String>;
    async fn load_page_hashes_unknown_page(
        &self,
        query: PageHashUnknownQuery,
    ) -> Result<PageHashPage<PageHashUnknownEntry>, String>;
    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> Result<Vec<PageHashDeleteTarget>, String>;
    async fn upsert_page_hash(&self, command: PageHashUpsertCommand) -> Result<(), String>;
}
