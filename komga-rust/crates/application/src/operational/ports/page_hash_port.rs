use async_trait::async_trait;

use crate::media_assets::{PageHashDeleteTarget, PageHashThumbnail};

#[async_trait]
pub trait PageHashPort: Send + Sync {
    async fn load_page_hash_matches_page(
        &self,
        page_hash: &str,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<serde_json::Value, String>;
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
        page: u64,
        size: u64,
        actions: &[String],
        sorts: &[String],
    ) -> Result<serde_json::Value, String>;
    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<serde_json::Value, String>;
    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> Result<Vec<PageHashDeleteTarget>, String>;
    async fn upsert_page_hash(
        &self,
        hash: &str,
        size: Option<i64>,
        action: &str,
    ) -> Result<(), String>;
}
