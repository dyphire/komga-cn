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
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        publishers: Vec<String>,
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String>;

    async fn load_browse_publisher_entries(
        &self,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
    ) -> Result<Vec<BrowsePublisherEntry>, String>;

    async fn load_keep_reading_books(
        &self,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_on_deck_books(
        &self,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_books(
        &self,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_books_paged(
        &self,
        allowed_library_ids: Option<HashSet<String>>,
        user_id: Option<String>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String>;

    async fn load_latest_series(
        &self,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_latest_series_paged(
        &self,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_library_series(
        &self,
        library_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_series_page(
        &self,
        allowed_library_ids: Option<HashSet<String>>,
        search: Option<String>,
        publishers: Vec<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String>;

    async fn load_all_readlists(&self) -> Result<Vec<OpdsReadlistEntry>, String>;
}

#[async_trait]
pub trait OpdsPersistedService: Send + Sync {
    async fn load_libraries(&self) -> Result<Vec<PersistedLibraryRecord>, String>;
    async fn load_library(
        &self,
        library_id: String,
    ) -> Result<Option<PersistedLibraryRecord>, String>;
    async fn load_readlists_for_library(
        &self,
        library_id: String,
    ) -> Result<Vec<PersistedReadlistRecord>, String>;
    async fn load_series(&self, series_id: String)
    -> Result<Option<PersistedSeriesRecord>, String>;
    async fn load_series_books_paged(
        &self,
        series_id: String,
        user_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String>;
    async fn load_series_tags(&self, series_id: String) -> Result<Vec<String>, String>;
    async fn load_readlist(
        &self,
        readlist_id: String,
    ) -> Result<Option<PersistedReadlistRecord>, String>;
    async fn load_readlist_books(
        &self,
        readlist_id: String,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String>;
    async fn load_unified_search_results(
        &self,
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
        allowed_library_ids: Option<HashSet<String>>,
    ) -> Result<Vec<String>, String>;
    async fn load_collections(
        &self,
        library_id: Option<String>,
    ) -> Result<Vec<PersistedNamedRecord>, String>;
    async fn load_collection(
        &self,
        collection_id: String,
    ) -> Result<Option<PersistedNamedRecord>, String>;
    async fn load_collection_books(
        &self,
        collection_id: String,
    ) -> Result<Vec<PersistedBookFeedRecord>, String>;
    async fn load_collection_series(
        &self,
        collection_id: String,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String>;
}
