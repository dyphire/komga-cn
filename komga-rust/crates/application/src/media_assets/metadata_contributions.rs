#[async_trait::async_trait]
pub trait SeriesMetadataContributionCleanupPort: Send + Sync {
    async fn delete_book_contributions(&self, book_ids: &[String]) -> anyhow::Result<()>;
}
