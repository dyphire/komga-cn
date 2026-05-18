use super::*;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct OpdsState {
    pub(crate) server_settings: Arc<dyn ServerSettingsService>,
    pub(crate) opds_catalog: Arc<dyn OpdsCatalogService>,
    pub(crate) opds_persisted: Arc<dyn OpdsPersistedService>,
    pub(crate) discovery_detail: Arc<dyn DiscoveryDetailService>,
    pub(crate) reader: komga_infrastructure::media_reader::MediaReader,
    pub(crate) content: komga_infrastructure::content_resolver::ContentResolver,
}

impl FromRef<Arc<HttpAppState>> for OpdsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            server_settings: app.services.server_settings.clone(),
            opds_catalog: app.services.opds_catalog.clone(),
            opds_persisted: app.services.opds_persisted.clone(),
            discovery_detail: app.services.discovery_detail.clone(),
            reader: app.services.media_reader.clone(),
            content: app.services.content_resolver.clone(),
        }
    }
}

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

    async fn load_all_readlists(&self) -> Result<Vec<OpdsReadlistEntry>, String>;
}

#[async_trait]
pub trait OpdsPersistedService: Send + Sync {
    async fn load_libraries(&self) -> Result<Vec<PersistedLibraryRecord>, String>;
    async fn load_library(
        &self,
        library_id: &str,
    ) -> Result<Option<PersistedLibraryRecord>, String>;
    async fn load_readlists_for_library(
        &self,
        library_id: &str,
    ) -> Result<Vec<PersistedReadlistRecord>, String>;
    async fn load_series(&self, series_id: &str) -> Result<Option<PersistedSeriesRecord>, String>;
    async fn load_series_books_paged(
        &self,
        series_id: &str,
        user_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String>;
    async fn load_series_tags(&self, series_id: &str) -> Result<Vec<String>, String>;
    async fn load_readlist(
        &self,
        readlist_id: &str,
    ) -> Result<Option<PersistedReadlistRecord>, String>;
    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String>;
    async fn load_unified_search_results(
        &self,
        query: &str,
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
        allowed_library_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<String>, String>;
    async fn load_collections(
        &self,
        library_id: Option<&str>,
    ) -> Result<Vec<PersistedNamedRecord>, String>;
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
