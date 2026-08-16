use std::collections::HashSet;

use super::records::{
    PersistedBookFeedRecord, PersistedBookSearchRecord, PersistedLibraryRecord,
    PersistedNamedRecord, PersistedReadlistBookRecord, PersistedReadlistRecord,
    PersistedSeriesBookRecord, PersistedSeriesRecord, PersistedSeriesSearchRecord,
};

#[derive(Clone, Default)]
pub struct OpdsPersistedUnifiedSearchRecords {
    pub series: Vec<PersistedSeriesSearchRecord>,
    pub books: Vec<PersistedBookSearchRecord>,
    pub collections: Vec<PersistedNamedRecord>,
    pub readlists: Vec<PersistedNamedRecord>,
}

/// Persisted library lookup used by OPDS composition services.
#[async_trait::async_trait]
pub trait OpdsLibraryPersistedPort: Send + Sync {
    async fn load_libraries(&self) -> anyhow::Result<Vec<PersistedLibraryRecord>>;

    async fn load_library(
        &self,
        library_id: &str,
    ) -> anyhow::Result<Option<PersistedLibraryRecord>>;
}

/// Persisted readlist visibility data used by OPDS browse and feed composition.
#[async_trait::async_trait]
pub trait OpdsReadlistVisibilityPersistedPort: Send + Sync {
    async fn load_readlists_for_library(
        &self,
        library_id: &str,
    ) -> anyhow::Result<Vec<PersistedReadlistRecord>>;

    async fn load_all_readlists(&self) -> anyhow::Result<Vec<PersistedReadlistRecord>>;

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<PersistedReadlistBookRecord>>;
}

/// Persisted collection visibility data used by OPDS browse and feed composition.
#[async_trait::async_trait]
pub trait OpdsCollectionVisibilityPersistedPort: Send + Sync {
    async fn load_collections(
        &self,
        library_id: Option<&str>,
    ) -> anyhow::Result<Vec<PersistedNamedRecord>>;

    async fn load_collection_series(
        &self,
        collection_id: &str,
        ordered: bool,
    ) -> anyhow::Result<Vec<PersistedSeriesRecord>>;
}

/// Persisted data needed to compose OPDS feed and recommended pages.
pub trait OpdsFeedPersistedPort:
    OpdsLibraryPersistedPort
    + OpdsReadlistVisibilityPersistedPort
    + OpdsCollectionVisibilityPersistedPort
{
}

impl<T> OpdsFeedPersistedPort for T where
    T: OpdsLibraryPersistedPort
        + OpdsReadlistVisibilityPersistedPort
        + OpdsCollectionVisibilityPersistedPort
        + ?Sized
{
}

#[async_trait::async_trait]
pub trait OpdsPublisherPersistedPort: Send + Sync {
    async fn load_publishers(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
    ) -> anyhow::Result<Vec<String>>;
}

#[async_trait::async_trait]
pub trait OpdsCollectionDetailPersistedPort: Send + Sync {
    async fn load_collection(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<PersistedNamedRecord>>;

    async fn load_collection_books(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<PersistedBookFeedRecord>>;

    async fn load_collection_series(
        &self,
        collection_id: &str,
        ordered: bool,
    ) -> anyhow::Result<Vec<PersistedSeriesRecord>>;
}

#[async_trait::async_trait]
pub trait OpdsReadlistDetailPersistedPort: Send + Sync {
    async fn load_readlist(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Option<PersistedReadlistRecord>>;

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<PersistedReadlistBookRecord>>;
}

#[async_trait::async_trait]
pub trait OpdsSeriesPersistedPort: Send + Sync {
    async fn load_series(&self, series_id: &str) -> anyhow::Result<Option<PersistedSeriesRecord>>;

    async fn load_series_books_paged(
        &self,
        series_id: &str,
        user_id: &str,
        offset: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<PersistedSeriesBookRecord>>;

    async fn load_series_tags(&self, series_id: &str) -> anyhow::Result<Vec<String>>;
}

#[async_trait::async_trait]
pub trait OpdsSearchPersistedPort: Send + Sync {
    async fn load_unified_search_results(
        &self,
        query: &str,
    ) -> anyhow::Result<OpdsPersistedUnifiedSearchRecords>;

    async fn load_collection_books(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<PersistedBookFeedRecord>>;

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<PersistedReadlistBookRecord>>;
}
