use komga_domain::discovery::{BookCondition, SeriesCondition};

#[derive(Clone, serde::Serialize)]
pub struct PersistedAuthorEntry {
    pub name: String,
    pub role: String,
}

#[derive(Clone, serde::Serialize)]
pub struct PersistedWebLinkEntry {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SeriesFilterCriteria {
    pub collection_ids: Option<Vec<String>>,
}

#[derive(Clone)]
pub(crate) struct PersistedSeriesBrowseQuery {
    pub filters: SeriesFilterCriteria,
    pub condition: Option<SeriesCondition>,
    pub search: Option<String>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub sort_modes: Vec<PersistedSeriesSortMode>,
}

impl PersistedSeriesBrowseQuery {
    pub fn from_filters(
        filters: SeriesFilterCriteria,
        search: Option<String>,
        page: usize,
        size: usize,
        unpaged: bool,
        sort_modes: Vec<PersistedSeriesSortMode>,
    ) -> Self {
        Self {
            filters,
            condition: None,
            search,
            page,
            size,
            unpaged,
            sort_modes,
        }
    }

    pub fn with_condition(mut self, condition: Option<SeriesCondition>) -> Self {
        self.condition = condition;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistedSeriesSortMode {
    TitleAsc,
    TitleDesc,
    NameAsc,
    NameDesc,
    ReadDateAsc,
    ReadDateDesc,
    CollectionNumberAsc,
    CollectionNumberDesc,
    Random,
    CreatedAsc,
    CreatedDesc,
    LastModifiedAsc,
    LastModifiedDesc,
    ReleaseDateAsc,
    ReleaseDateDesc,
    BooksCountAsc,
    BooksCountDesc,
    RelevanceAsc,
    RelevanceDesc,
}

#[derive(Clone)]
pub struct PersistedSeriesSummary {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
    pub title_sort: String,
    pub labels: Vec<String>,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub books_count: u64,
    pub books_read_count: u64,
    pub books_unread_count: u64,
    pub books_in_progress_count: u64,
    pub status: String,
    pub summary: String,
    pub reading_direction: String,
    pub publisher: String,
    pub age_rating: Option<u16>,
    pub language: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub alternate_titles: Vec<String>,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub books_metadata_authors: Vec<String>,
    pub books_metadata_tags: Vec<String>,
    pub books_metadata_release_date: Option<String>,
    pub books_metadata_summary: String,
    pub books_metadata_summary_number: String,
    pub books_metadata_created: String,
    pub books_metadata_last_modified: String,
    pub deleted: bool,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BooksFilterCriteria {
    pub library_ids: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistedBooksSortMode {
    TitleAsc,
    TitleDesc,
    NameAsc,
    NameDesc,
    SeriesTitleAsc,
    SeriesTitleDesc,
    CreatedDateAsc,
    CreatedDateDesc,
    LastModifiedDateAsc,
    LastModifiedDateDesc,
    FileSizeAsc,
    FileSizeDesc,
    FileHashAsc,
    FileHashDesc,
    UrlAsc,
    UrlDesc,
    MediaStatusAsc,
    MediaStatusDesc,
    MediaCommentAsc,
    MediaCommentDesc,
    MediaTypeAsc,
    MediaTypeDesc,
    MediaPagesCountAsc,
    MediaPagesCountDesc,
    ReadProgressLastModifiedDateAsc,
    ReadProgressLastModifiedDateDesc,
    ReadProgressReadDateAsc,
    ReadProgressReadDateDesc,
    ReleaseDateAsc,
    ReleaseDateDesc,
    NumberSortAsc,
    NumberSortDesc,
    SeriesIdAsc,
    ReadListNumberAsc,
    ReadListNumberDesc,
    RelevanceAsc,
    RelevanceDesc,
}

#[derive(Clone)]
pub(crate) struct PersistedBooksBrowseQuery {
    pub filters: BooksFilterCriteria,
    pub condition: Option<BookCondition>,
    pub search: Option<String>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub sort_modes: Vec<PersistedBooksSortMode>,
}

impl PersistedBooksBrowseQuery {
    pub fn from_filters(
        filters: BooksFilterCriteria,
        search: Option<String>,
        page: usize,
        size: usize,
        unpaged: bool,
        sort_modes: Vec<PersistedBooksSortMode>,
    ) -> Self {
        Self {
            filters,
            condition: None,
            search,
            page,
            size,
            unpaged,
            sort_modes,
        }
    }

    pub fn with_condition(mut self, condition: Option<BookCondition>) -> Self {
        self.condition = condition;
        self
    }
}

#[derive(Clone)]
pub struct PersistedBookSummary {
    pub id: String,
    pub series_id: String,
    pub library_id: String,
    pub series_title: String,
    pub series_title_sort: String,
    pub title: String,
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
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub read_status: String,
    pub metadata_title_lock: bool,
    pub metadata_summary: String,
    pub metadata_summary_lock: bool,
    pub metadata_number: String,
    pub metadata_number_lock: bool,
    pub metadata_number_sort: f64,
    pub metadata_number_sort_lock: bool,
    pub metadata_release_date: Option<String>,
    pub metadata_release_date_lock: bool,
    pub metadata_authors_lock: bool,
    pub metadata_tags_lock: bool,
    pub metadata_isbn: String,
    pub metadata_isbn_lock: bool,
    pub metadata_links_lock: bool,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub file_hash: String,
    pub read_progress: Option<PersistedReadProgressSummary>,
    pub deleted: bool,
    pub oneshot: bool,
    pub genres: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub age_rating: Option<u16>,
    pub metadata_tags: Vec<String>,
    pub metadata_authors: Vec<PersistedAuthorEntry>,
    pub metadata_links: Vec<PersistedWebLinkEntry>,
}

#[derive(Clone)]
pub struct PersistedReadProgressSummary {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone)]
pub struct PersistedBookPosterSummary {
    pub thumbnail_type: String,
    pub selected: bool,
}
