use std::collections::HashMap;

use async_trait::async_trait;

use super::read_models::{BookMetadataAuthorReadModel, BookReadModel, SeriesReadModel};

// --- Book record types ---

#[derive(Clone)]
pub struct PersistedBookResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u16>,
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
    pub age_rating: Option<u16>,
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
    pub status: String,
    pub summary: String,
    pub reading_direction: String,
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

pub struct ExistingSeriesMetadataRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
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

#[derive(Clone)]
pub struct SeriesMetadataLinkRecord {
    pub label: String,
    pub url: String,
}

#[derive(Clone)]
pub struct SeriesAlternateTitleRecord {
    pub label: String,
    pub title: String,
}

pub struct SeriesMetadataUpdateRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
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

#[async_trait]
pub trait ReadlistBookPort: Send + Sync {
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String>;

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<BookReadModel>, String>;

    async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<BookMetadataAuthorReadModel>, String>;
}

#[async_trait]
pub trait BookDetailPort: Send + Sync {
    async fn load_book_id_by_sorted_position(&self, index: usize)
    -> Result<Option<String>, String>;

    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String>;

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<BookReadModel>, String>;

    async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String>;

    async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<BookMetadataAuthorReadModel>, String>;
}

#[async_trait]
impl<T> ReadlistBookPort for T
where
    T: BookDetailPort + ?Sized,
{
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        BookDetailPort::load_persisted_book_resource(self, book_id).await
    }

    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<BookReadModel>, String> {
        BookDetailPort::load_persisted_book_detail(self, book_id, user_id).await
    }

    async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<BookMetadataAuthorReadModel>, String> {
        BookDetailPort::load_persisted_book_authors(self, book_id).await
    }
}

#[async_trait]
pub trait SeriesDetailPort: Send + Sync {
    async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String>;

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String>;

    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String>;

    async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String>;

    async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String>;

    async fn load_persisted_series_summaries(&self) -> Result<Vec<SeriesReadModel>, String>;

    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String>;

    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String>;

    async fn load_persisted_series_collections(
        &self,
        series_id: &str,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String>;

    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String>;

    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String>;

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait CollectionSeriesPort: Send + Sync {
    async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String>;

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String>;
}

#[async_trait]
impl<T> CollectionSeriesPort for T
where
    T: SeriesDetailPort + ?Sized,
{
    async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String> {
        SeriesDetailPort::load_series_library_id(self, series_id).await
    }

    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        SeriesDetailPort::load_series_restrictions(self, series_id).await
    }
}

#[async_trait]
pub trait CollectionPort: Send + Sync {
    async fn persisted_collections_exist(&self) -> Result<bool, String>;

    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String>;

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<(), String>;

    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<bool, String>;

    async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String>;

    async fn upsert_collection_search_document(&self, collection_id: &str) -> Result<bool, String>;

    async fn delete_collection_search_document(&self, collection_id: &str) -> Result<(), String>;
}

#[async_trait]
pub trait CollectionListPort: Send + Sync {
    async fn persisted_collections_exist(&self) -> Result<bool, String>;

    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String>;
}

#[async_trait]
impl<T> CollectionListPort for T
where
    T: CollectionPort + ?Sized,
{
    async fn persisted_collections_exist(&self) -> Result<bool, String> {
        CollectionPort::persisted_collections_exist(self).await
    }

    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        CollectionPort::load_persisted_collections(self).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        CollectionPort::load_persisted_collection_series_ids(self, collection_id).await
    }
}

#[async_trait]
pub trait CollectionDetailPort: Send + Sync {
    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String>;
}

#[async_trait]
impl<T> CollectionDetailPort for T
where
    T: CollectionPort + ?Sized,
{
    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        CollectionPort::load_persisted_collection_detail(self, collection_id).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        CollectionPort::load_persisted_collection_series_ids(self, collection_id).await
    }
}

#[async_trait]
pub trait CollectionMutationPort: Send + Sync {
    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String>;

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<(), String>;

    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<bool, String>;

    async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String>;

    async fn upsert_collection_search_document(&self, collection_id: &str) -> Result<bool, String>;

    async fn delete_collection_search_document(&self, collection_id: &str) -> Result<(), String>;
}

#[async_trait]
impl<T> CollectionMutationPort for T
where
    T: CollectionPort + ?Sized,
{
    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        CollectionPort::load_persisted_collections(self).await
    }

    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        CollectionPort::load_persisted_collection_detail(self, collection_id).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        CollectionPort::load_persisted_collection_series_ids(self, collection_id).await
    }

    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<(), String> {
        CollectionPort::persist_collection_create(self, collection_id, name, ordered, series_ids)
            .await
    }

    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<bool, String> {
        CollectionPort::persist_collection_update(self, collection_id, name, ordered, series_ids)
            .await
    }

    async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String> {
        CollectionPort::delete_persisted_collection(self, collection_id).await
    }

    async fn upsert_collection_search_document(&self, collection_id: &str) -> Result<bool, String> {
        CollectionPort::upsert_collection_search_document(self, collection_id).await
    }

    async fn delete_collection_search_document(&self, collection_id: &str) -> Result<(), String> {
        CollectionPort::delete_collection_search_document(self, collection_id).await
    }
}

#[async_trait]
pub trait ReadlistPort: Send + Sync {
    async fn load_persisted_readlists(
        &self,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String>;

    async fn load_comicrack_match_candidates(
        &self,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String>;

    async fn persist_readlist_create(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<(), String>;

    async fn persist_readlist_update(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<bool, String>;

    async fn delete_persisted_readlist(&self, readlist_id: &str) -> Result<bool, String>;

    async fn upsert_readlist_search_document(&self, readlist_id: &str) -> Result<bool, String>;

    async fn delete_readlist_search_document(&self, readlist_id: &str) -> Result<(), String>;
}
