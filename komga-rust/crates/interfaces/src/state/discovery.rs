use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[async_trait]
pub trait DiscoveryAuthorService: Send + Sync {
    async fn load_author_names(
        &self,
        search: String,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;

    async fn load_author_roles(
        &self,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;

    async fn load_authors_by_scope(
        &self,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<PersistedAuthorEntry>, String>;
}

#[async_trait]
pub trait DiscoveryLibraryMappingService: Send + Sync {
    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String>;
}

#[async_trait]
pub trait DiscoveryCollectionSearchService: Send + Sync {
    async fn search_collection_ids(
        &self,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String>;
}

#[async_trait]
pub trait DiscoveryReadlistSearchService: Send + Sync {
    async fn search_readlist_scored_ids(
        &self,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;
}

#[async_trait]
pub trait DiscoveryBookFeedService: Send + Sync {
    async fn load_ondeck_books(
        &self,
        user_id: String,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String>;

    async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String>;
}

#[async_trait]
pub trait PersistedDiscoveryListDataSource: Send + Sync {
    async fn load_book_poster_summaries(
        &self,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String>;

    async fn load_persisted_book_summaries(
        &self,
        user_id: Option<String>,
    ) -> Result<Vec<PersistedBookSummary>, String>;

    async fn load_persisted_book_summaries_by_ids(
        &self,
        user_id: Option<String>,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedBookSummary>, String>;

    async fn load_persisted_book_count(&self) -> Result<usize, String>;

    async fn load_persisted_genres(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_tags(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_languages(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_publishers(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_age_ratings(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_sharing_labels(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_series_release_dates(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_series_tags(
        &self,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;

    async fn load_collection_memberships(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String>;

    async fn load_collection_ordering(
        &self,
        collection_id: String,
    ) -> Result<HashMap<String, i64>, String>;

    async fn load_readlist_memberships(&self)
    -> Result<BTreeMap<String, BTreeSet<String>>, String>;

    async fn load_persisted_book_tags(
        &self,
        scope: Option<PersistedBookTagsScope>,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;

    async fn persisted_utc_date_minus_days(&self, days: i64) -> Result<Option<String>, String>;

    async fn load_series_read_progress_counts(
        &self,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String>;

    async fn load_series_read_dates(
        &self,
        user_id: String,
    ) -> Result<HashMap<String, String>, String>;

    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String>;

    async fn load_persisted_series_summaries(&self) -> Result<Vec<PersistedSeriesSummary>, String>;

    async fn load_persisted_series_summaries_by_ids(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedSeriesSummary>, String>;

    async fn load_persisted_series_count(&self) -> Result<usize, String>;

    async fn search_book_ids(&self, query: String, limit: usize) -> Result<Vec<String>, String>;

    async fn search_series_scored_ids(
        &self,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;
}

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

#[derive(Clone)]
pub struct PersistedBookResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedBookDetailRecord {
    pub id: String,
    pub series_id: String,
    pub series_title: String,
    pub series_title_sort: String,
    pub library_id: String,
    pub name: String,
    pub url: String,
    pub number: i32,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub size_bytes: u64,
    pub media_status: String,
    pub media_type: String,
    pub media_pages_count: u32,
    pub media_comment: String,
    pub metadata_title: String,
    pub metadata_summary: String,
    pub metadata_number: String,
    pub metadata_number_sort: f64,
    pub metadata_release_date: Option<String>,
    pub metadata_title_lock: bool,
    pub metadata_summary_lock: bool,
    pub metadata_number_lock: bool,
    pub metadata_number_sort_lock: bool,
    pub metadata_release_date_lock: bool,
    pub metadata_authors: String,
    pub metadata_authors_lock: bool,
    pub metadata_tags: String,
    pub metadata_tags_lock: bool,
    pub metadata_isbn: String,
    pub metadata_isbn_lock: bool,
    pub metadata_links: String,
    pub metadata_links_lock: bool,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub read_progress: Option<DiscoveryPersistedReadProgressRecord>,
    pub deleted: bool,
    pub file_hash: String,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct DiscoveryPersistedReadProgressRecord {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Copy)]
pub enum PersistedBookSiblingDirectionRecord {
    Previous,
    Next,
}

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

#[derive(Clone)]
pub struct PersistedBookAuthorRecord {
    pub name: String,
    pub role: String,
}

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

#[derive(Clone)]
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

#[derive(Clone)]
pub struct SeriesSummaryRecord {
    pub id: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub alternate_titles: Vec<String>,
    pub books_metadata_authors: Vec<String>,
    pub books_metadata_tags: Vec<String>,
    pub books_metadata_release_date: Option<String>,
    pub books_metadata_summary: String,
    pub books_metadata_summary_number: String,
    pub books_metadata_created: String,
    pub books_metadata_last_modified: String,
}

#[async_trait]
pub trait DiscoveryDetailService: Send + Sync {
    async fn load_book_id_by_sorted_position(&self, index: usize)
    -> Result<Option<String>, String>;

    async fn load_persisted_book_resource(
        &self,
        book_id: String,
    ) -> Result<Option<PersistedBookResourceRecord>, String>;

    async fn load_persisted_book_detail(
        &self,
        book_id: String,
        user_id: Option<String>,
    ) -> Result<Option<PersistedBookDetailRecord>, String>;

    async fn load_persisted_book_sibling_id(
        &self,
        book_id: String,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String>;

    async fn persisted_collections_exist(&self) -> Result<bool, String>;

    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: String,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_collection_detail(
        &self,
        collection_id: String,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String>;

    async fn load_series_library_id(&self, series_id: String) -> Result<Option<String>, String>;

    async fn load_series_restrictions(
        &self,
        series_id: String,
    ) -> Result<PersistedSeriesRestrictionRecord, String>;

    async fn persist_collection_create(
        &self,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<(), String>;

    async fn persist_collection_update(
        &self,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<bool, String>;

    async fn delete_persisted_collection(&self, collection_id: String) -> Result<bool, String>;

    async fn upsert_collection_search_document(
        &self,
        collection_id: String,
    ) -> Result<bool, String>;

    async fn delete_collection_search_document(&self, collection_id: String) -> Result<(), String>;

    async fn load_persisted_readlists(
        &self,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: String,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: String,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String>;

    async fn load_comicrack_match_candidates(
        &self,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String>;

    async fn load_persisted_book_authors(
        &self,
        book_id: String,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String>;

    async fn persist_readlist_create(
        &self,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<(), String>;

    async fn persist_readlist_update(
        &self,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<bool, String>;

    async fn delete_persisted_readlist(&self, readlist_id: String) -> Result<bool, String>;

    async fn upsert_readlist_search_document(&self, readlist_id: String) -> Result<bool, String>;

    async fn delete_readlist_search_document(&self, readlist_id: String) -> Result<(), String>;

    async fn load_persisted_series_resource(
        &self,
        series_id: String,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String>;

    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String>;

    async fn load_persisted_series_detail(
        &self,
        series_id: String,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String>;

    async fn load_persisted_series_summaries(&self) -> Result<Vec<SeriesSummaryRecord>, String>;

    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String>;

    async fn load_series_read_progress_counts(
        &self,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String>;

    async fn load_persisted_series_collections(
        &self,
        series_id: String,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String>;

    async fn load_existing_series_metadata(
        &self,
        series_id: String,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String>;

    async fn persist_series_metadata_update(
        &self,
        series_id: String,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String>;

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: String,
    ) -> Result<(), String>;
}
