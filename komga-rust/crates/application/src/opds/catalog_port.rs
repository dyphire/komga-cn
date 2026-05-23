use std::collections::HashSet;

use async_trait::async_trait;

use super::records::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookFeedEntry, OpdsSeriesEntry,
};

/// Port for OPDS catalog browsing operations.
#[async_trait]
pub trait OpdsCatalogPort: Send + Sync {
    async fn load_browse_series_navigation_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        publishers: &[String],
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String>;

    async fn load_browse_publisher_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
    ) -> Result<Vec<BrowsePublisherEntry>, String>;

    async fn load_keep_reading_books(
        &self,
        user_id: &str,
        library_id: Option<&str>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_on_deck_books(
        &self,
        user_id: &str,
        library_id: Option<&str>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_books(
        &self,
        library_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_books_paged(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        user_id: Option<&str>,
        library_id: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_series(
        &self,
        library_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_latest_series_paged(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_library_series(
        &self,
        library_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_series_page(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        search: Option<&str>,
        publishers: &[String],
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;
}
