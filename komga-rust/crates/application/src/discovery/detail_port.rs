use komga_domain::discovery::SeriesStatus;
use std::collections::HashMap;

use super::browse_engine::SeriesReadProgressCounts;
use super::read_models::{BookReadModel, SeriesReadModel};
use super::reading_direction::SeriesReadingDirection;

// --- Book record types ---

#[derive(Clone)]
pub struct PersistedBookResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u32>,
    pub sharing_labels: String,
}

#[derive(Clone, Copy)]
pub enum PersistedBookSiblingDirectionRecord {
    Previous,
    Next,
}

// --- Collection record types ---

#[derive(Clone)]
pub struct PersistedCollectionAccessRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct PersistedSeriesRestrictionRecord {
    pub age_rating: Option<u32>,
    pub labels: Vec<String>,
}

// --- Readlist record types ---

#[derive(Clone)]
pub struct DiscoveryPersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

#[derive(Clone)]
pub struct DiscoveryPersistedReadlistBookRecord {
    pub book_id: String,
    pub library_id: String,
}

#[derive(Clone)]
pub struct PersistedComicrackMatchCandidateRecord {
    pub series_id: String,
    pub series_title: String,
    pub series_release_date: Option<String>,
    pub book_id: String,
    pub book_title: String,
    pub book_number: String,
}

// --- Series record types ---

#[derive(Clone)]
pub struct PersistedSeriesResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u32>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedSeriesDetailRecord {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
    pub title_sort: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub books_count: u32,
    pub status: SeriesStatus,
    pub summary: String,
    pub reading_direction: Option<SeriesReadingDirection>,
    pub publisher: String,
    pub age_rating: Option<u32>,
    pub language: String,
    pub sharing_labels: String,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub deleted: bool,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedSeriesCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExistingSeriesMetadataRecord {
    pub status: SeriesStatus,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<SeriesReadingDirection>,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesMetadataLinkRecord {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesAlternateTitleRecord {
    pub label: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesMetadataUpdateRecord {
    pub status: SeriesStatus,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<SeriesReadingDirection>,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

// --- Port traits ---

#[async_trait::async_trait]
pub trait ReadlistBookPort: Send + Sync {
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<PersistedBookResourceRecord>>;

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> anyhow::Result<Option<BookReadModel>>;
}

#[async_trait::async_trait]
pub trait BookDetailPort: Send + Sync {
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<PersistedBookResourceRecord>>;

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> anyhow::Result<Option<BookReadModel>>;

    async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> anyhow::Result<Option<String>>;
}

#[async_trait::async_trait]
pub trait PersistedBookIdResolverPort: Send + Sync {
    async fn persisted_book_resource_exists(&self, book_id: &str) -> anyhow::Result<bool>;

    async fn load_book_id_by_sorted_position(&self, index: usize)
    -> anyhow::Result<Option<String>>;
}

pub async fn resolve_persisted_book_id<B>(book_ids: &B, requested_book_id: &str) -> String
where
    B: PersistedBookIdResolverPort + ?Sized,
{
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };

    if index == 0 {
        return requested_book_id.to_string();
    }

    if matches!(
        book_ids
            .persisted_book_resource_exists(requested_book_id)
            .await,
        Ok(true)
    ) {
        return requested_book_id.to_string();
    }

    match book_ids.load_book_id_by_sorted_position(index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

#[async_trait::async_trait]
impl<T> ReadlistBookPort for T
where
    T: BookDetailPort + ?Sized,
{
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<PersistedBookResourceRecord>> {
        BookDetailPort::load_persisted_book_resource(self, book_id).await
    }

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> anyhow::Result<Option<BookReadModel>> {
        BookDetailPort::load_persisted_book_detail(self, book_id, user_id).await
    }
}

#[async_trait::async_trait]
pub trait SeriesDetailPort: Send + Sync {
    async fn load_series_library_id(&self, series_id: &str) -> anyhow::Result<Option<String>>;

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> anyhow::Result<PersistedSeriesRestrictionRecord>;

    async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<PersistedSeriesResourceRecord>>;

    async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<PersistedSeriesDetailRecord>>;

    async fn load_persisted_series_summaries(&self) -> anyhow::Result<Vec<SeriesReadModel>>;

    async fn load_series_total_book_counts(&self) -> anyhow::Result<HashMap<String, i64>>;

    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> anyhow::Result<HashMap<String, SeriesReadProgressCounts>>;

    async fn load_persisted_series_collections(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<PersistedSeriesCollectionRecord>>;

    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<ExistingSeriesMetadataRecord>>;
}

#[async_trait::async_trait]
pub trait PersistedSeriesIdResolverPort: Send + Sync {
    async fn persisted_series_resource_exists(&self, series_id: &str) -> anyhow::Result<bool>;

    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> anyhow::Result<Option<String>>;
}

pub async fn resolve_persisted_series_id<S>(series_ids: &S, requested_series_id: &str) -> String
where
    S: PersistedSeriesIdResolverPort + ?Sized,
{
    let Some(index) = requested_series_id
        .strip_prefix("series-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_series_id.to_string();
    };

    if index == 0 {
        return requested_series_id.to_string();
    }

    if matches!(
        series_ids
            .persisted_series_resource_exists(requested_series_id)
            .await,
        Ok(true)
    ) {
        return requested_series_id.to_string();
    }

    match series_ids.load_series_id_by_sorted_position(index).await {
        Ok(Some(series_id)) => series_id,
        _ => requested_series_id.to_string(),
    }
}

#[async_trait::async_trait]
pub trait CollectionSeriesPort: Send + Sync {
    async fn load_series_library_id(&self, series_id: &str) -> anyhow::Result<Option<String>>;

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> anyhow::Result<PersistedSeriesRestrictionRecord>;
}

#[async_trait::async_trait]
impl<T> CollectionSeriesPort for T
where
    T: SeriesDetailPort + ?Sized,
{
    async fn load_series_library_id(&self, series_id: &str) -> anyhow::Result<Option<String>> {
        SeriesDetailPort::load_series_library_id(self, series_id).await
    }

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> anyhow::Result<PersistedSeriesRestrictionRecord> {
        SeriesDetailPort::load_series_restrictions(self, series_id).await
    }
}

#[async_trait::async_trait]
pub trait CollectionPort: Send + Sync {
    async fn persisted_collections_exist(&self) -> anyhow::Result<bool>;

    async fn load_persisted_collections(
        &self,
    ) -> anyhow::Result<Vec<PersistedCollectionAccessRecord>>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>>;

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<PersistedCollectionAccessRecord>>;

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> anyhow::Result<()>;

    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> anyhow::Result<bool>;

    async fn delete_persisted_collection(&self, collection_id: &str) -> anyhow::Result<bool>;

    async fn upsert_collection_search_document(&self, collection_id: &str) -> anyhow::Result<bool>;

    async fn delete_collection_search_document(&self, collection_id: &str) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait CollectionProjectionPort: Send + Sync {
    async fn persisted_collections_exist(&self) -> anyhow::Result<bool>;

    async fn load_persisted_collections(
        &self,
    ) -> anyhow::Result<Vec<PersistedCollectionAccessRecord>>;

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<PersistedCollectionAccessRecord>>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>>;
}

#[async_trait::async_trait]
impl<T> CollectionProjectionPort for T
where
    T: CollectionPort + ?Sized,
{
    async fn persisted_collections_exist(&self) -> anyhow::Result<bool> {
        CollectionPort::persisted_collections_exist(self).await
    }

    async fn load_persisted_collections(
        &self,
    ) -> anyhow::Result<Vec<PersistedCollectionAccessRecord>> {
        CollectionPort::load_persisted_collections(self).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        CollectionPort::load_persisted_collection_series_ids(self, collection_id).await
    }

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<PersistedCollectionAccessRecord>> {
        CollectionPort::load_persisted_collection_detail(self, collection_id).await
    }
}

#[async_trait::async_trait]
pub trait CollectionMutationPort: Send + Sync {
    async fn load_persisted_collections(
        &self,
    ) -> anyhow::Result<Vec<PersistedCollectionAccessRecord>>;

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<PersistedCollectionAccessRecord>>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>>;

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> anyhow::Result<()>;

    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> anyhow::Result<bool>;

    async fn delete_persisted_collection(&self, collection_id: &str) -> anyhow::Result<bool>;

    async fn upsert_collection_search_document(&self, collection_id: &str) -> anyhow::Result<bool>;

    async fn delete_collection_search_document(&self, collection_id: &str) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl<T> CollectionMutationPort for T
where
    T: CollectionPort + ?Sized,
{
    async fn load_persisted_collections(
        &self,
    ) -> anyhow::Result<Vec<PersistedCollectionAccessRecord>> {
        CollectionPort::load_persisted_collections(self).await
    }

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Option<PersistedCollectionAccessRecord>> {
        CollectionPort::load_persisted_collection_detail(self, collection_id).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        CollectionPort::load_persisted_collection_series_ids(self, collection_id).await
    }

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> anyhow::Result<()> {
        CollectionPort::persist_collection_create(self, collection_id, name, ordered, series_ids)
            .await
    }

    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> anyhow::Result<bool> {
        CollectionPort::persist_collection_update(self, collection_id, name, ordered, series_ids)
            .await
    }

    async fn delete_persisted_collection(&self, collection_id: &str) -> anyhow::Result<bool> {
        CollectionPort::delete_persisted_collection(self, collection_id).await
    }

    async fn upsert_collection_search_document(&self, collection_id: &str) -> anyhow::Result<bool> {
        CollectionPort::upsert_collection_search_document(self, collection_id).await
    }

    async fn delete_collection_search_document(&self, collection_id: &str) -> anyhow::Result<()> {
        CollectionPort::delete_collection_search_document(self, collection_id).await
    }
}

#[async_trait::async_trait]
pub trait ReadlistProjectionPort: Send + Sync {
    async fn load_persisted_readlists(
        &self,
    ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistRecord>>;

    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Option<DiscoveryPersistedReadlistRecord>>;

    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistBookRecord>>;
}

#[async_trait::async_trait]
pub trait ReadlistComicRackMatchPort: Send + Sync {
    async fn load_persisted_readlists(
        &self,
    ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistRecord>>;

    async fn load_comicrack_match_candidates(
        &self,
    ) -> anyhow::Result<Vec<PersistedComicrackMatchCandidateRecord>>;
}

#[async_trait::async_trait]
pub trait ReadlistMutationPort: ReadlistProjectionPort {
    async fn persist_readlist_create(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> anyhow::Result<()>;

    async fn persist_readlist_update(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> anyhow::Result<bool>;

    async fn delete_persisted_readlist(&self, readlist_id: &str) -> anyhow::Result<bool>;

    async fn upsert_readlist_search_document(&self, readlist_id: &str) -> anyhow::Result<bool>;

    async fn delete_readlist_search_document(&self, readlist_id: &str) -> anyhow::Result<()>;
}
