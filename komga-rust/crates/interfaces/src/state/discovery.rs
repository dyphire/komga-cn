use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[async_trait]
pub trait PersistedDiscoveryService: Send + Sync {
    async fn load_persisted_author_names(
        &self,
        database_file: PathBuf,
        search: String,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_author_roles(
        &self,
        database_file: PathBuf,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_authors_by_scope(
        &self,
        database_file: PathBuf,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<PersistedAuthorEntry>, String>;
    async fn load_book_poster_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String>;
    async fn load_persisted_book_summaries(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Result<Vec<PersistedBookSummary>, String>;
    async fn load_persisted_book_summaries_by_ids(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedBookSummary>, String>;
    async fn load_persisted_book_count(&self, database_file: PathBuf) -> Result<usize, String>;
    async fn load_persisted_genres(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_languages(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_publishers(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_age_ratings(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_sharing_labels(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_series_release_dates(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_series_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_library_ids(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<String>, String>;
    async fn load_collection_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String>;
    async fn load_collection_ordering(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<HashMap<String, i64>, String>;
    async fn load_readlist_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String>;
    async fn load_persisted_ondeck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String>;
    async fn load_persisted_duplicate_books(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String>;
    async fn load_persisted_book_tags(
        &self,
        database_file: PathBuf,
        scope: Option<PersistedBookTagsScope>,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;
    async fn persisted_utc_date_minus_days(
        &self,
        database_file: PathBuf,
        days: i64,
    ) -> Result<Option<String>, String>;
    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String>;
    async fn load_series_read_dates(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, String>, String>;
    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String>;
    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedSeriesSummary>, String>;
    async fn load_persisted_series_summaries_by_ids(
        &self,
        database_file: PathBuf,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedSeriesSummary>, String>;
    async fn load_persisted_series_count(&self, database_file: PathBuf) -> Result<usize, String>;
    async fn persisted_series_exist(&self, database_file: PathBuf) -> Result<bool, String>;
    async fn search_book_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String>;
    async fn search_collection_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String>;
    async fn search_readlist_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;
    async fn search_series_scored_ids(
        &self,
        database_file: PathBuf,
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
    async fn load_book_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String>;

    async fn load_persisted_book_resource(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<PersistedBookResourceRecord>, String>;

    async fn load_persisted_book_detail(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: Option<String>,
    ) -> Result<Option<PersistedBookDetailRecord>, String>;

    async fn load_persisted_book_sibling_id(
        &self,
        database_file: PathBuf,
        book_id: String,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String>;

    async fn persisted_collections_exist(&self, database_file: PathBuf) -> Result<bool, String>;

    async fn load_persisted_collections(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String>;

    async fn load_persisted_collection_series_ids(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<String>, String>;

    async fn load_persisted_collection_detail(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String>;

    async fn load_series_library_id(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<String>, String>;

    async fn load_series_restrictions(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<PersistedSeriesRestrictionRecord, String>;

    async fn persist_collection_create(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<(), String>;

    async fn persist_collection_update(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<bool, String>;

    async fn delete_persisted_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String>;

    async fn upsert_collection_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<bool, String>;

    async fn delete_collection_search_document(
        &self,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<(), String>;

    async fn load_persisted_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_detail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String>;

    async fn load_persisted_readlist_book_rows(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String>;

    async fn load_comicrack_match_candidates(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String>;

    async fn load_persisted_book_authors(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String>;

    async fn persist_readlist_create(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<(), String>;

    async fn persist_readlist_update(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<bool, String>;

    async fn delete_persisted_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String>;

    async fn upsert_readlist_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String>;

    async fn delete_readlist_search_document(
        &self,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<(), String>;

    async fn load_persisted_series_resource(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String>;

    async fn load_series_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String>;

    async fn load_persisted_series_detail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String>;

    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<SeriesSummaryRecord>, String>;

    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String>;

    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String>;

    async fn load_persisted_series_collections(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String>;

    async fn load_existing_series_metadata(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String>;

    async fn persist_series_metadata_update(
        &self,
        database_file: PathBuf,
        series_id: String,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String>;

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        series_id: String,
    ) -> Result<(), String>;
}

#[async_trait]
impl<T> DiscoveryDetailService for Arc<T>
where
    T: DiscoveryDetailService + ?Sized,
{
    async fn load_book_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_book_id_by_sorted_position(database_file, index)
            .await
    }

    async fn load_persisted_book_resource(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        (**self)
            .load_persisted_book_resource(database_file, book_id)
            .await
    }

    async fn load_persisted_book_detail(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: Option<String>,
    ) -> Result<Option<PersistedBookDetailRecord>, String> {
        (**self)
            .load_persisted_book_detail(database_file, book_id, user_id)
            .await
    }

    async fn load_persisted_book_sibling_id(
        &self,
        database_file: PathBuf,
        book_id: String,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_persisted_book_sibling_id(database_file, book_id, direction)
            .await
    }

    async fn persisted_collections_exist(&self, database_file: PathBuf) -> Result<bool, String> {
        (**self).persisted_collections_exist(database_file).await
    }

    async fn load_persisted_collections(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        (**self).load_persisted_collections(database_file).await
    }

    async fn load_persisted_collection_series_ids(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_collection_series_ids(database_file, collection_id)
            .await
    }

    async fn load_persisted_collection_detail(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        (**self)
            .load_persisted_collection_detail(database_file, collection_id)
            .await
    }

    async fn load_series_library_id(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_series_library_id(database_file, series_id)
            .await
    }

    async fn load_series_restrictions(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        (**self)
            .load_series_restrictions(database_file, series_id)
            .await
    }

    async fn persist_collection_create(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<(), String> {
        (**self)
            .persist_collection_create(database_file, collection_id, name, ordered, series_ids)
            .await
    }

    async fn persist_collection_update(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<bool, String> {
        (**self)
            .persist_collection_update(database_file, collection_id, name, ordered, series_ids)
            .await
    }

    async fn delete_persisted_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_persisted_collection(database_file, collection_id)
            .await
    }

    async fn upsert_collection_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        (**self)
            .upsert_collection_search_document(database_file, index_dir, collection_id)
            .await
    }

    async fn delete_collection_search_document(
        &self,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<(), String> {
        (**self)
            .delete_collection_search_document(index_dir, collection_id)
            .await
    }

    async fn load_persisted_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        (**self).load_persisted_readlists(database_file).await
    }

    async fn load_persisted_readlist_detail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        (**self)
            .load_persisted_readlist_detail(database_file, readlist_id)
            .await
    }

    async fn load_persisted_readlist_book_rows(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        (**self)
            .load_persisted_readlist_book_rows(database_file, readlist_id)
            .await
    }

    async fn load_comicrack_match_candidates(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        (**self)
            .load_comicrack_match_candidates(database_file)
            .await
    }

    async fn load_persisted_book_authors(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String> {
        (**self)
            .load_persisted_book_authors(database_file, book_id)
            .await
    }

    async fn persist_readlist_create(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<(), String> {
        (**self)
            .persist_readlist_create(database_file, readlist_id, name, summary, ordered, book_ids)
            .await
    }

    async fn persist_readlist_update(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<bool, String> {
        (**self)
            .persist_readlist_update(database_file, readlist_id, name, summary, ordered, book_ids)
            .await
    }

    async fn delete_persisted_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_persisted_readlist(database_file, readlist_id)
            .await
    }

    async fn upsert_readlist_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        (**self)
            .upsert_readlist_search_document(database_file, index_dir, readlist_id)
            .await
    }

    async fn delete_readlist_search_document(
        &self,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<(), String> {
        (**self)
            .delete_readlist_search_document(index_dir, readlist_id)
            .await
    }

    async fn load_persisted_series_resource(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        (**self)
            .load_persisted_series_resource(database_file, series_id)
            .await
    }

    async fn load_series_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_series_id_by_sorted_position(database_file, index)
            .await
    }

    async fn load_persisted_series_detail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        (**self)
            .load_persisted_series_detail(database_file, series_id)
            .await
    }

    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<SeriesSummaryRecord>, String> {
        (**self)
            .load_persisted_series_summaries(database_file)
            .await
    }

    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String> {
        (**self).load_series_total_book_counts(database_file).await
    }

    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        (**self)
            .load_series_read_progress_counts(database_file, user_id)
            .await
    }

    async fn load_persisted_series_collections(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
        (**self)
            .load_persisted_series_collections(database_file, series_id)
            .await
    }

    async fn load_existing_series_metadata(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        (**self)
            .load_existing_series_metadata(database_file, series_id)
            .await
    }

    async fn persist_series_metadata_update(
        &self,
        database_file: PathBuf,
        series_id: String,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        (**self)
            .persist_series_metadata_update(database_file, series_id, update)
            .await
    }

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        series_id: String,
    ) -> Result<(), String> {
        (**self)
            .refresh_series_search_documents_after_metadata_update(
                database_file,
                index_dir,
                series_id,
            )
            .await
    }
}
