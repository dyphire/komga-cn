use std::collections::HashSet;

use super::feed_context::{OpdsFeedUserContext, OpdsPagedBooks, OpdsPagedSeries};
use super::records::{BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsSeriesEntry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpdsBookFeedKind {
    KeepReading,
    OnDeck,
    LatestBooks { include_read_progress: bool },
}

pub struct OpdsBookFeedQuery<'a> {
    pub user: &'a OpdsFeedUserContext,
    pub library_id: Option<&'a str>,
    pub page: usize,
    pub size: usize,
    pub kind: OpdsBookFeedKind,
}

pub struct OpdsLatestSeriesFeedQuery<'a> {
    pub user: &'a OpdsFeedUserContext,
    pub library_id: Option<&'a str>,
    pub page: usize,
    pub size: usize,
    pub include_one_shots: bool,
}

pub struct OpdsLibrarySeriesQuery<'a> {
    pub user: &'a OpdsFeedUserContext,
    pub library_id: &'a str,
    pub page: usize,
    pub size: usize,
}

pub struct OpdsSeriesFeedPage {
    pub series: Vec<OpdsSeriesEntry>,
    pub has_next: bool,
}

pub struct BrowseSeriesNavigationPage {
    pub entries: Vec<BrowseSeriesNavigationEntry>,
    pub total_count: usize,
}

/// Port for OPDS feed queries.
#[async_trait::async_trait]
pub trait OpdsFeedCatalogPort: Send + Sync {
    async fn load_book_feed_page(
        &self,
        query: OpdsBookFeedQuery<'_>,
    ) -> anyhow::Result<OpdsPagedBooks>;

    async fn load_latest_series_feed_page(
        &self,
        query: OpdsLatestSeriesFeedQuery<'_>,
    ) -> anyhow::Result<OpdsPagedSeries>;

    async fn load_library_series_feed_page(
        &self,
        query: OpdsLibrarySeriesQuery<'_>,
    ) -> anyhow::Result<OpdsSeriesFeedPage>;
}

/// Port for OPDS catalog browsing operations.
#[async_trait::async_trait]
pub trait OpdsBrowseCatalogPort: Send + Sync {
    async fn load_browse_series_navigation_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        publishers: &[String],
        page: usize,
        size: usize,
    ) -> anyhow::Result<BrowseSeriesNavigationPage>;

    async fn load_browse_publisher_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
    ) -> anyhow::Result<Vec<BrowsePublisherEntry>>;

    async fn load_series_page(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        search: Option<&str>,
        publishers: &[String],
        offset: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<OpdsSeriesEntry>>;
}
