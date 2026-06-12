use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::super::reading_direction::SeriesReadingDirection;
use komga_domain::discovery::{
    BookCondition, MediaStatus, QueryRestrictions, ReadStatus, SeriesCondition, SeriesStatus,
};
use komga_domain::media_assets::ThumbnailType;

#[derive(Clone)]
pub struct AuthorEntry {
    pub name: String,
    pub role: String,
}

#[derive(Clone)]
pub struct WebLinkEntry {
    pub label: String,
    pub url: String,
}

#[derive(Clone)]
pub struct ReadProgressRow {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone)]
pub struct BookPosterRow {
    pub thumbnail_type: ThumbnailType,
    pub selected: bool,
}

#[derive(Clone)]
pub struct BookRow {
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
    pub media_status: MediaStatus,
    pub media_type: String,
    pub media_pages_count: u32,
    pub media_comment: String,
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub read_status: ReadStatus,
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
    pub read_progress: Option<ReadProgressRow>,
    pub deleted: bool,
    pub oneshot: bool,
    pub genres: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub age_rating: Option<u32>,
    pub metadata_tags: Vec<String>,
    pub metadata_authors: Vec<AuthorEntry>,
    pub metadata_links: Vec<WebLinkEntry>,
}

#[derive(Clone)]
pub struct SeriesRow {
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
    pub status: SeriesStatus,
    pub summary: String,
    pub reading_direction: Option<SeriesReadingDirection>,
    pub publisher: String,
    pub age_rating: Option<u32>,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeriesReadProgressCounts {
    pub read_count: i64,
    pub in_progress_count: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookSortMode {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeriesSortMode {
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

pub struct BookBrowseQuery {
    pub condition: Option<BookCondition>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub sort_modes: Vec<BookSortMode>,
    pub relevance_ranks: HashMap<String, usize>,
    pub readlist_order: HashMap<String, usize>,
}

pub struct SeriesBrowseQuery {
    pub condition: Option<SeriesCondition>,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub sort_modes: Vec<SeriesSortMode>,
    pub relevance_ranks: HashMap<String, usize>,
    pub collection_order: HashMap<String, usize>,
}

pub struct BrowseContext {
    pub user_id: Option<String>,
    pub is_admin: bool,
    pub authorized_library_ids: Option<Vec<String>>,
    pub restrictions: Option<QueryRestrictions>,
}

pub struct BookEvaluationContext {
    pub user_id_present: bool,
    pub readlist_memberships: Option<BTreeMap<String, BTreeSet<String>>>,
    pub posters: Option<HashMap<String, Vec<BookPosterRow>>>,
    pub release_date_cutoffs: HashMap<i64, Option<String>>,
}

pub struct SeriesEvaluationContext {
    pub user_id_present: bool,
    pub collection_memberships: Option<BTreeMap<String, BTreeSet<String>>>,
    pub read_progress: Option<HashMap<String, SeriesReadProgressCounts>>,
    pub total_book_counts: Option<HashMap<String, i64>>,
    pub read_dates: Option<HashMap<String, String>>,
    pub release_date_cutoffs: HashMap<i64, Option<String>>,
}

pub struct PageEnvelope<T> {
    pub content: Vec<T>,
    pub page: usize,
    pub page_size: usize,
    pub total_elements: usize,
}
