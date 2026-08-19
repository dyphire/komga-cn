use komga_domain::discovery::{BookCondition, MediaStatus, ReadStatus, SeriesCondition};
use komga_domain::media_assets::ThumbnailType;

#[derive(Clone, serde::Serialize)]
pub(super) struct PersistedAuthorEntry {
    pub(super) name: String,
    pub(super) role: String,
}

#[derive(Clone, serde::Serialize)]
pub(super) struct PersistedWebLinkEntry {
    pub(super) label: String,
    pub(super) url: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SeriesFilterCriteria {
    pub(super) collection_ids: Option<Vec<String>>,
}

#[derive(Clone)]
pub(super) struct PersistedSeriesBrowseQuery {
    pub(super) filters: SeriesFilterCriteria,
    pub(super) condition: Option<SeriesCondition>,
    pub(super) search: Option<String>,
    pub(super) page: usize,
    pub(super) size: usize,
    pub(super) unpaged: bool,
    pub(super) sort_modes: Vec<PersistedSeriesSortMode>,
}

impl PersistedSeriesBrowseQuery {
    pub(super) fn from_filters(
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

    pub(super) fn with_condition(mut self, condition: Option<SeriesCondition>) -> Self {
        self.condition = condition;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistedSeriesSortMode {
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
pub(super) struct PersistedSeriesSummary {
    pub(super) id: String,
    pub(super) library_id: String,
    pub(super) name: String,
    pub(super) title: String,
    pub(super) title_sort: String,
    pub(super) labels: Vec<String>,
    pub(super) created: String,
    pub(super) last_modified: String,
    pub(super) file_last_modified: String,
    pub(super) books_count: u64,
    pub(super) books_read_count: u64,
    pub(super) books_unread_count: u64,
    pub(super) books_in_progress_count: u64,
    pub(super) status: String,
    pub(super) summary: String,
    pub(super) reading_direction: String,
    pub(super) publisher: String,
    pub(super) age_rating: Option<u32>,
    pub(super) language: String,
    pub(super) genres: Vec<String>,
    pub(super) tags: Vec<String>,
    pub(super) alternate_titles: Vec<String>,
    pub(super) metadata_created: String,
    pub(super) metadata_last_modified: String,
    pub(super) books_metadata_authors: Vec<String>,
    pub(super) books_metadata_tags: Vec<String>,
    pub(super) books_metadata_release_date: Option<String>,
    pub(super) books_metadata_summary: String,
    pub(super) books_metadata_summary_number: String,
    pub(super) books_metadata_created: String,
    pub(super) books_metadata_last_modified: String,
    pub(super) deleted: bool,
    pub(super) oneshot: bool,
    pub(super) status_lock: bool,
    pub(super) title_lock: bool,
    pub(super) title_sort_lock: bool,
    pub(super) summary_lock: bool,
    pub(super) reading_direction_lock: bool,
    pub(super) publisher_lock: bool,
    pub(super) age_rating_lock: bool,
    pub(super) language_lock: bool,
    pub(super) genres_lock: bool,
    pub(super) tags_lock: bool,
    pub(super) total_book_count_lock: bool,
    pub(super) sharing_labels_lock: bool,
    pub(super) links_lock: bool,
    pub(super) alternate_titles_lock: bool,
    pub(super) links: Vec<PersistedWebLinkEntry>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BooksFilterCriteria {
    pub(super) library_ids: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistedBooksSortMode {
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
pub(super) struct PersistedBooksBrowseQuery {
    pub(super) filters: BooksFilterCriteria,
    pub(super) condition: Option<BookCondition>,
    pub(super) search: Option<String>,
    pub(super) page: usize,
    pub(super) size: usize,
    pub(super) unpaged: bool,
    pub(super) sort_modes: Vec<PersistedBooksSortMode>,
}

impl PersistedBooksBrowseQuery {
    pub(super) fn from_filters(
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

    pub(super) fn with_condition(mut self, condition: Option<BookCondition>) -> Self {
        self.condition = condition;
        self
    }
}

#[derive(Clone)]
pub(super) struct PersistedBookSummary {
    pub(super) id: String,
    pub(super) series_id: String,
    pub(super) library_id: String,
    pub(super) series_title: String,
    pub(super) series_title_sort: String,
    pub(super) title: String,
    pub(super) name: String,
    pub(super) url: String,
    pub(super) number: i32,
    pub(super) created: String,
    pub(super) last_modified: String,
    pub(super) file_last_modified: String,
    pub(super) size_bytes: u64,
    pub(super) media_status: MediaStatus,
    pub(super) media_type: String,
    pub(super) media_pages_count: u32,
    pub(super) media_comment: String,
    pub(super) media_epub_divina_compatible: bool,
    pub(super) media_epub_is_kepub: bool,
    pub(super) read_status: ReadStatus,
    pub(super) metadata_title_lock: bool,
    pub(super) metadata_summary: String,
    pub(super) metadata_summary_lock: bool,
    pub(super) metadata_number: String,
    pub(super) metadata_number_lock: bool,
    pub(super) metadata_number_sort: f64,
    pub(super) metadata_number_sort_lock: bool,
    pub(super) metadata_release_date: Option<String>,
    pub(super) metadata_release_date_lock: bool,
    pub(super) metadata_authors_lock: bool,
    pub(super) metadata_tags_lock: bool,
    pub(super) metadata_isbn: String,
    pub(super) metadata_isbn_lock: bool,
    pub(super) metadata_links_lock: bool,
    pub(super) metadata_created: String,
    pub(super) metadata_last_modified: String,
    pub(super) file_hash: String,
    pub(super) read_progress: Option<PersistedReadProgressSummary>,
    pub(super) deleted: bool,
    pub(super) oneshot: bool,
    pub(super) genres: Vec<String>,
    pub(super) language: Option<String>,
    pub(super) publisher: Option<String>,
    pub(super) age_rating: Option<u32>,
    pub(super) metadata_tags: Vec<String>,
    pub(super) metadata_authors: Vec<PersistedAuthorEntry>,
    pub(super) metadata_links: Vec<PersistedWebLinkEntry>,
}

#[derive(Clone)]
pub(super) struct PersistedReadProgressSummary {
    pub(super) page: i32,
    pub(super) completed: bool,
    pub(super) read_date: Option<String>,
    pub(super) created: String,
    pub(super) last_modified: String,
    pub(super) device_id: String,
    pub(super) device_name: String,
}

#[derive(Clone)]
pub(super) struct PersistedBookPosterSummary {
    pub(super) thumbnail_type: ThumbnailType,
    pub(super) selected: bool,
}
