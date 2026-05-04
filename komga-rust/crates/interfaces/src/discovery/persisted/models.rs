use std::ops::{Deref, DerefMut};

#[derive(Clone)]
pub struct PersistedBookBrowseEntry {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
}

#[derive(Clone)]
pub enum PersistedBookTagsScope {
    All,
    Series(String),
    Libraries(Vec<String>),
    ReadList(String),
}

#[derive(Clone)]
pub enum PersistedAuthorsScope {
    All,
    Libraries(Vec<String>),
    Collection(String),
    Series(String),
    ReadList(String),
}

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
pub struct SeriesFilterCriteria {
    pub library_ids: Option<Vec<String>>,
    pub collection_ids: Option<Vec<String>>,
    pub titles: Option<Vec<String>>,
    pub titles_excluded: Option<Vec<String>>,
    pub titles_contains: Option<Vec<String>>,
    pub titles_contains_excluded: Option<Vec<String>>,
    pub titles_begins_with: Option<Vec<String>>,
    pub titles_begins_with_excluded: Option<Vec<String>>,
    pub titles_ends_with: Option<Vec<String>>,
    pub titles_ends_with_excluded: Option<Vec<String>>,
    pub titles_regex: Option<Vec<String>>,
    pub title_sorts: Option<Vec<String>>,
    pub title_sorts_excluded: Option<Vec<String>>,
    pub title_sorts_contains: Option<Vec<String>>,
    pub title_sorts_contains_excluded: Option<Vec<String>>,
    pub title_sorts_begins_with: Option<Vec<String>>,
    pub title_sorts_begins_with_excluded: Option<Vec<String>>,
    pub title_sorts_ends_with: Option<Vec<String>>,
    pub title_sorts_ends_with_excluded: Option<Vec<String>>,
    pub title_sorts_regex: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub exclude_newly_added: bool,
    pub read_statuses: Option<Vec<String>>,
    pub read_statuses_excluded: Option<Vec<String>>,
    pub genres: Option<Vec<String>>,
    pub genres_excluded: Option<Vec<String>>,
    pub genres_null: Option<bool>,
    pub languages: Option<Vec<String>>,
    pub languages_excluded: Option<Vec<String>>,
    pub publishers: Option<Vec<String>>,
    pub publishers_excluded: Option<Vec<String>>,
    pub age_ratings: Option<Vec<u16>>,
    pub age_ratings_or_empty: Option<Vec<u16>>,
    pub age_ratings_excluded: Option<Vec<u16>>,
    pub age_ratings_null: Option<bool>,
    pub age_rating_gt: Option<u16>,
    pub age_rating_lt: Option<u16>,
    pub tags: Option<Vec<String>>,
    pub tags_excluded: Option<Vec<String>>,
    pub tags_null: Option<bool>,
    pub release_dates: Option<Vec<String>>,
    pub release_dates_excluded: Option<Vec<String>>,
    pub release_dates_null: Option<bool>,
    pub release_date_gt: Option<String>,
    pub release_date_lt: Option<String>,
    pub release_date_begins_with: Option<Vec<String>>,
    pub release_date_ends_with: Option<Vec<String>>,
    pub release_date_contains_excluded: Option<Vec<String>>,
    pub release_date_begins_with_excluded: Option<Vec<String>>,
    pub release_date_ends_with_excluded: Option<Vec<String>>,
    pub release_date_in_last_days: Option<i64>,
    pub release_date_not_in_last_days: Option<i64>,
    pub sharing_labels: Option<Vec<String>>,
    pub sharing_labels_contains: Option<Vec<String>>,
    pub sharing_labels_excluded: Option<Vec<String>>,
    pub sharing_labels_null: Option<bool>,
    pub series_statuses: Option<Vec<String>>,
    pub series_statuses_excluded: Option<Vec<String>>,
    pub complete: Option<bool>,
    pub authors_contains: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub authors_excluded: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct PersistedSeriesBrowseQuery {
    pub filters: SeriesFilterCriteria,
    pub sharing_labels_contains_groups: Vec<Vec<String>>,
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
            sharing_labels_contains_groups: vec![],
            search,
            page,
            size,
            unpaged,
            sort_modes,
        }
    }
}

impl Deref for PersistedSeriesBrowseQuery {
    type Target = SeriesFilterCriteria;

    fn deref(&self) -> &Self::Target {
        &self.filters
    }
}

impl DerefMut for PersistedSeriesBrowseQuery {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.filters
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistedSeriesSortMode {
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
pub struct BooksFilterCriteria {
    pub library_ids: Option<Vec<String>>,
    pub series_ids: Option<Vec<String>>,
    pub series_ids_excluded: Option<Vec<String>>,
    pub read_list_ids: Option<Vec<String>>,
    pub read_list_ids_excluded: Option<Vec<String>>,
    pub titles: Option<Vec<String>>,
    pub titles_excluded: Option<Vec<String>>,
    pub titles_contains: Option<Vec<String>>,
    pub titles_contains_excluded: Option<Vec<String>>,
    pub titles_begins_with: Option<Vec<String>>,
    pub titles_begins_with_excluded: Option<Vec<String>>,
    pub titles_ends_with: Option<Vec<String>>,
    pub titles_ends_with_excluded: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub genres: Option<Vec<String>>,
    pub genres_excluded: Option<Vec<String>>,
    pub genres_null: Option<bool>,
    pub languages: Option<Vec<String>>,
    pub languages_excluded: Option<Vec<String>>,
    pub publishers: Option<Vec<String>>,
    pub publishers_excluded: Option<Vec<String>>,
    pub age_ratings: Option<Vec<u16>>,
    pub age_ratings_excluded: Option<Vec<u16>>,
    pub age_ratings_null: Option<bool>,
    pub age_rating_gt: Option<u16>,
    pub age_rating_lt: Option<u16>,
    pub tags: Option<Vec<String>>,
    pub tags_excluded: Option<Vec<String>>,
    pub tags_null: Option<bool>,
    pub read_statuses: Option<Vec<String>>,
    pub read_statuses_excluded: Option<Vec<String>>,
    pub media_profiles: Option<Vec<String>>,
    pub media_profiles_excluded: Option<Vec<String>>,
    pub media_statuses: Option<Vec<String>>,
    pub media_statuses_excluded: Option<Vec<String>>,
    pub authors_contains: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub authors_excluded: Option<Vec<String>>,
    pub poster_types: Option<Vec<String>>,
    pub poster_types_excluded: Option<Vec<String>>,
    pub poster_selected: Option<bool>,
    pub poster_selected_excluded: Option<bool>,
    pub release_dates: Option<Vec<String>>,
    pub release_dates_excluded: Option<Vec<String>>,
    pub release_dates_null: Option<bool>,
    pub release_date_gt: Option<String>,
    pub release_date_lt: Option<String>,
    pub release_date_begins_with: Option<Vec<String>>,
    pub release_date_ends_with: Option<Vec<String>>,
    pub release_date_contains_excluded: Option<Vec<String>>,
    pub release_date_begins_with_excluded: Option<Vec<String>>,
    pub release_date_ends_with_excluded: Option<Vec<String>>,
    pub release_date_in_last_days: Option<i64>,
    pub release_date_not_in_last_days: Option<i64>,
    pub number_sorts: Option<Vec<f64>>,
    pub number_sorts_excluded: Option<Vec<f64>>,
    pub number_sort_gt: Option<f64>,
    pub number_sort_lt: Option<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistedBooksSortMode {
    TitleAsc,
    CreatedDateDesc,
    LastModifiedDateDesc,
    ReadProgressLastModifiedDateAsc,
    ReadProgressLastModifiedDateDesc,
    ReadProgressReadDateAsc,
    ReadProgressReadDateDesc,
    ReleaseDateDesc,
    NumberSortAsc,
    SeriesIdAsc,
    RelevanceAsc,
    RelevanceDesc,
}

#[derive(Clone)]
pub struct PersistedBooksBrowseQuery {
    pub filters: BooksFilterCriteria,
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
            search,
            page,
            size,
            unpaged,
            sort_modes,
        }
    }
}

impl Deref for PersistedBooksBrowseQuery {
    type Target = BooksFilterCriteria;

    fn deref(&self) -> &Self::Target {
        &self.filters
    }
}

impl DerefMut for PersistedBooksBrowseQuery {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.filters
    }
}

#[derive(Clone)]
pub struct PersistedBookSummary {
    pub id: String,
    pub series_id: String,
    pub library_id: String,
    pub series_title: String,
    pub title: String,
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
