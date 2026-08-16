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
    ) -> anyhow::Result<PageHashPage<PageHashMatchEntry>>;
    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> anyhow::Result<Option<PageHashThumbnail>>;
    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> anyhow::Result<Option<PageHashThumbnail>>;
    async fn load_page_hashes_page(
        &self,
        query: PageHashKnownQuery,
    ) -> anyhow::Result<PageHashPage<PageHashKnownEntry>>;
    async fn load_page_hashes_unknown_page(
        &self,
        query: PageHashUnknownQuery,
    ) -> anyhow::Result<PageHashPage<PageHashUnknownEntry>>;
    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> anyhow::Result<Vec<PageHashDeleteTarget>>;
    async fn upsert_page_hash(&self, command: PageHashUpsertCommand) -> anyhow::Result<()>;
}
