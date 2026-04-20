use super::*;

pub struct BrowseSeriesNavigationEntry {
    pub id: String,
    pub title: String,
}

pub struct BrowsePublisherEntry {
    pub publisher: String,
}

pub struct OpdsBookAuthorEntry {
    pub name: String,
    pub role: String,
}

pub struct OpdsBookFeedEntry {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<OpdsBookAuthorEntry>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub last_read: Option<i64>,
    pub last_read_date: Option<String>,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct OpdsSeriesEntry {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub one_shot: bool,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct OpdsReadlistEntry {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

pub struct PersistedLibraryRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

pub struct PersistedSeriesRecord {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub summary: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct PersistedSeriesBookRecord {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<OpdsPersistedBookAuthorRecord>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub last_read: Option<i64>,
    pub last_read_date: Option<String>,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct PersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
    pub ordered: bool,
}

pub struct OpdsPersistedBookAuthorRecord {
    pub name: String,
    pub role: String,
}

pub struct PersistedReadlistBookRecord {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<OpdsPersistedBookAuthorRecord>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub media_status: Option<String>,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct PersistedSeriesSearchRecord {
    pub id: String,
    pub title: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct PersistedBookSearchRecord {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<OpdsPersistedBookAuthorRecord>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct PersistedNamedRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
    pub ordered: bool,
}

pub struct PersistedBookFeedRecord {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub media_type: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

#[async_trait]
pub trait OpdsCatalogService: Send + Sync {
    async fn load_browse_series_navigation_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        publishers: Vec<String>,
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String>;

    async fn load_browse_publisher_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
    ) -> Result<Vec<BrowsePublisherEntry>, String>;

    async fn load_keep_reading_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_on_deck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_books(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_books_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        user_id: Option<String>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_series(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_latest_series_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_library_series(
        &self,
        database_file: PathBuf,
        library_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_series_page(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        search: Option<String>,
        publishers: Vec<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_all_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<OpdsReadlistEntry>, String>;
}

#[async_trait]
pub trait OpdsPersistedService: Send + Sync {
    async fn load_libraries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedLibraryRecord>, String>;
    async fn load_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Option<PersistedLibraryRecord>, String>;
    async fn load_readlists_for_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Vec<PersistedReadlistRecord>, String>;
    async fn load_series(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesRecord>, String>;
    async fn load_series_books_paged(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String>;
    async fn load_series_tags(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<String>, String>;
    async fn load_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<PersistedReadlistRecord>, String>;
    async fn load_readlist_books(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String>;
    async fn load_unified_search_results(
        &self,
        database_file: PathBuf,
        query: String,
    ) -> Result<
        (
            Vec<PersistedSeriesSearchRecord>,
            Vec<PersistedBookSearchRecord>,
            Vec<PersistedNamedRecord>,
            Vec<PersistedNamedRecord>,
        ),
        String,
    >;
    async fn load_publishers(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
    ) -> Result<Vec<String>, String>;
    async fn load_collections(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
    ) -> Result<Vec<PersistedNamedRecord>, String>;
    async fn load_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedNamedRecord>, String>;
    async fn load_collection_books(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<PersistedBookFeedRecord>, String>;
    async fn load_collection_series(
        &self,
        database_file: PathBuf,
        collection_id: String,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String>;
}

#[async_trait]
impl<T> PersistedDiscoveryService for Arc<T>
where
    T: PersistedDiscoveryService + ?Sized,
{
    async fn load_persisted_author_names(
        &self,
        database_file: PathBuf,
        search: String,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_author_names(database_file, search, authorized_library_ids)
            .await
    }

    async fn load_persisted_author_roles(
        &self,
        database_file: PathBuf,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_author_roles(database_file, authorized_library_ids)
            .await
    }

    async fn load_persisted_authors_by_scope(
        &self,
        database_file: PathBuf,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<PersistedAuthorEntry>, String> {
        (**self)
            .load_persisted_authors_by_scope(database_file, scope, authorized_library_ids)
            .await
    }

    async fn load_book_poster_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
        (**self).load_book_poster_summaries(database_file).await
    }

    async fn load_persisted_book_summaries(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        (**self)
            .load_persisted_book_summaries(database_file, user_id)
            .await
    }

    async fn load_persisted_book_summaries_by_ids(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        (**self)
            .load_persisted_book_summaries_by_ids(database_file, user_id, ids)
            .await
    }

    async fn load_persisted_book_count(&self, database_file: PathBuf) -> Result<usize, String> {
        (**self).load_persisted_book_count(database_file).await
    }

    async fn load_persisted_genres(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_genres(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_tags(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_languages(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_languages(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_publishers(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_publishers(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_age_ratings(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_age_ratings(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_sharing_labels(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_sharing_labels(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_series_release_dates(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_series_release_dates(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_series_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_series_tags(database_file, library_ids, collection_id)
            .await
    }

    async fn load_persisted_library_ids(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<String>, String> {
        (**self).load_persisted_library_ids(database_file).await
    }

    async fn load_collection_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        (**self).load_collection_memberships(database_file).await
    }

    async fn load_collection_ordering(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<HashMap<String, i64>, String> {
        (**self)
            .load_collection_ordering(database_file, collection_id)
            .await
    }

    async fn load_readlist_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        (**self).load_readlist_memberships(database_file).await
    }

    async fn load_persisted_ondeck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        (**self)
            .load_persisted_ondeck_books(database_file, user_id)
            .await
    }

    async fn load_persisted_duplicate_books(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        (**self).load_persisted_duplicate_books(database_file).await
    }

    async fn load_persisted_book_tags(
        &self,
        database_file: PathBuf,
        scope: Option<PersistedBookTagsScope>,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_book_tags(database_file, scope, authorized_library_ids)
            .await
    }

    async fn persisted_utc_date_minus_days(
        &self,
        database_file: PathBuf,
        days: i64,
    ) -> Result<Option<String>, String> {
        (**self)
            .persisted_utc_date_minus_days(database_file, days)
            .await
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

    async fn load_series_read_dates(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, String>, String> {
        (**self)
            .load_series_read_dates(database_file, user_id)
            .await
    }

    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String> {
        (**self).load_series_total_book_counts(database_file).await
    }

    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        (**self)
            .load_persisted_series_summaries(database_file)
            .await
    }

    async fn load_persisted_series_summaries_by_ids(
        &self,
        database_file: PathBuf,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        (**self)
            .load_persisted_series_summaries_by_ids(database_file, ids)
            .await
    }

    async fn load_persisted_series_count(&self, database_file: PathBuf) -> Result<usize, String> {
        (**self).load_persisted_series_count(database_file).await
    }

    async fn persisted_series_exist(&self, database_file: PathBuf) -> Result<bool, String> {
        (**self).persisted_series_exist(database_file).await
    }

    async fn search_book_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        (**self).search_book_ids(database_file, query, limit).await
    }

    async fn search_collection_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        (**self)
            .search_collection_ids(database_file, query, limit)
            .await
    }

    async fn search_readlist_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        (**self)
            .search_readlist_scored_ids(database_file, query, limit)
            .await
    }

    async fn search_series_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        (**self)
            .search_series_scored_ids(database_file, query, limit)
            .await
    }
}

#[async_trait]
impl<T> OpdsCatalogService for Arc<T>
where
    T: OpdsCatalogService + ?Sized,
{
    async fn load_browse_series_navigation_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        publishers: Vec<String>,
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String> {
        (**self)
            .load_browse_series_navigation_entries(
                database_file,
                allowed_library_ids,
                library_id,
                publishers,
                page,
                size,
            )
            .await
    }

    async fn load_browse_publisher_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
    ) -> Result<Vec<BrowsePublisherEntry>, String> {
        (**self)
            .load_browse_publisher_entries(database_file, allowed_library_ids, library_id)
            .await
    }

    async fn load_keep_reading_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        (**self)
            .load_keep_reading_books(database_file, user_id, library_id)
            .await
    }

    async fn load_on_deck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        (**self)
            .load_on_deck_books(database_file, user_id, library_id)
            .await
    }

    async fn load_latest_books(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        (**self)
            .load_latest_books(database_file, library_id, limit)
            .await
    }

    async fn load_latest_books_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        user_id: Option<String>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        (**self)
            .load_latest_books_paged(
                database_file,
                allowed_library_ids,
                user_id,
                library_id,
                offset,
                limit,
            )
            .await
    }

    async fn load_latest_series(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        (**self)
            .load_latest_series(database_file, library_id, limit)
            .await
    }

    async fn load_latest_series_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        (**self)
            .load_latest_series_paged(
                database_file,
                allowed_library_ids,
                library_id,
                offset,
                limit,
            )
            .await
    }

    async fn load_library_series(
        &self,
        database_file: PathBuf,
        library_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        (**self)
            .load_library_series(database_file, library_id, offset, limit)
            .await
    }

    async fn load_series_page(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        search: Option<String>,
        publishers: Vec<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        (**self)
            .load_series_page(
                database_file,
                allowed_library_ids,
                search,
                publishers,
                offset,
                limit,
            )
            .await
    }

    async fn load_all_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<OpdsReadlistEntry>, String> {
        (**self).load_all_readlists(database_file).await
    }
}

#[async_trait]
impl<T> OpdsPersistedService for Arc<T>
where
    T: OpdsPersistedService + ?Sized,
{
    async fn load_libraries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedLibraryRecord>, String> {
        (**self).load_libraries(database_file).await
    }

    async fn load_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Option<PersistedLibraryRecord>, String> {
        (**self).load_library(database_file, library_id).await
    }

    async fn load_readlists_for_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Vec<PersistedReadlistRecord>, String> {
        (**self)
            .load_readlists_for_library(database_file, library_id)
            .await
    }

    async fn load_series(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesRecord>, String> {
        (**self).load_series(database_file, series_id).await
    }

    async fn load_series_books_paged(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String> {
        (**self)
            .load_series_books_paged(database_file, series_id, user_id, offset, limit)
            .await
    }

    async fn load_series_tags(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<String>, String> {
        (**self).load_series_tags(database_file, series_id).await
    }

    async fn load_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<PersistedReadlistRecord>, String> {
        (**self).load_readlist(database_file, readlist_id).await
    }

    async fn load_readlist_books(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String> {
        (**self)
            .load_readlist_books(database_file, readlist_id)
            .await
    }

    async fn load_unified_search_results(
        &self,
        database_file: PathBuf,
        query: String,
    ) -> Result<
        (
            Vec<PersistedSeriesSearchRecord>,
            Vec<PersistedBookSearchRecord>,
            Vec<PersistedNamedRecord>,
            Vec<PersistedNamedRecord>,
        ),
        String,
    > {
        (**self)
            .load_unified_search_results(database_file, query)
            .await
    }

    async fn load_publishers(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_publishers(database_file, allowed_library_ids)
            .await
    }

    async fn load_collections(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
    ) -> Result<Vec<PersistedNamedRecord>, String> {
        (**self).load_collections(database_file, library_id).await
    }

    async fn load_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedNamedRecord>, String> {
        (**self).load_collection(database_file, collection_id).await
    }

    async fn load_collection_books(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<PersistedBookFeedRecord>, String> {
        (**self)
            .load_collection_books(database_file, collection_id)
            .await
    }

    async fn load_collection_series(
        &self,
        database_file: PathBuf,
        collection_id: String,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String> {
        (**self)
            .load_collection_series(database_file, collection_id, ordered)
            .await
    }
}
