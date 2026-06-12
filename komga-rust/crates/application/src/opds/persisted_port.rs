use std::collections::HashSet;

use async_trait::async_trait;

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
#[async_trait]
pub trait OpdsLibraryPersistedPort: Send + Sync {
    async fn load_libraries(&self) -> Result<Vec<PersistedLibraryRecord>, String>;

    async fn load_library(
        &self,
        library_id: &str,
    ) -> Result<Option<PersistedLibraryRecord>, String>;
}

/// Persisted readlist visibility data used by OPDS browse and feed composition.
#[async_trait]
pub trait OpdsReadlistVisibilityPersistedPort: Send + Sync {
    async fn load_readlists_for_library(
        &self,
        library_id: &str,
    ) -> Result<Vec<PersistedReadlistRecord>, String>;

    async fn load_all_readlists(&self) -> Result<Vec<PersistedReadlistRecord>, String>;

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String>;
}

/// Persisted collection visibility data used by OPDS browse and feed composition.
#[async_trait]
pub trait OpdsCollectionVisibilityPersistedPort: Send + Sync {
    async fn load_collections(
        &self,
        library_id: Option<&str>,
    ) -> Result<Vec<PersistedNamedRecord>, String>;

    async fn load_collection_series(
        &self,
        collection_id: &str,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String>;
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

#[async_trait]
pub trait OpdsPublisherPersistedPort: Send + Sync {
    async fn load_publishers(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<String>, String>;
}

#[async_trait]
pub trait OpdsCollectionDetailPersistedPort: Send + Sync {
    async fn load_collection(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedNamedRecord>, String>;

    async fn load_collection_books(
        &self,
        collection_id: &str,
    ) -> Result<Vec<PersistedBookFeedRecord>, String>;

    async fn load_collection_series(
        &self,
        collection_id: &str,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String>;
}

#[async_trait]
pub trait OpdsReadlistDetailPersistedPort: Send + Sync {
    async fn load_readlist(
        &self,
        readlist_id: &str,
    ) -> Result<Option<PersistedReadlistRecord>, String>;

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String>;
}

#[async_trait]
pub trait OpdsSeriesPersistedPort: Send + Sync {
    async fn load_series(&self, series_id: &str) -> Result<Option<PersistedSeriesRecord>, String>;

    async fn load_series_books_paged(
        &self,
        series_id: &str,
        user_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String>;

    async fn load_series_tags(&self, series_id: &str) -> Result<Vec<String>, String>;
}

#[async_trait]
pub trait OpdsSearchPersistedPort: Send + Sync {
    async fn load_unified_search_results(
        &self,
        query: &str,
    ) -> Result<OpdsPersistedUnifiedSearchRecords, String>;

    async fn load_collection_books(
        &self,
        collection_id: &str,
    ) -> Result<Vec<PersistedBookFeedRecord>, String>;

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String>;
}
