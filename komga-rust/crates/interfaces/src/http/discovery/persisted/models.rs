use super::*;
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

#[derive(Clone, Debug, Default)]
pub(crate) struct SeriesFilterCriteria {
    pub(crate) library_ids: Option<Vec<String>>,
    pub(crate) collection_ids: Option<Vec<String>>,
    pub(crate) titles: Option<Vec<String>>,
    pub(crate) titles_excluded: Option<Vec<String>>,
    pub(crate) titles_contains: Option<Vec<String>>,
    pub(crate) titles_contains_excluded: Option<Vec<String>>,
    pub(crate) titles_begins_with: Option<Vec<String>>,
    pub(crate) titles_begins_with_excluded: Option<Vec<String>>,
    pub(crate) titles_ends_with: Option<Vec<String>>,
    pub(crate) titles_ends_with_excluded: Option<Vec<String>>,
    pub(crate) title_sorts: Option<Vec<String>>,
    pub(crate) title_sorts_excluded: Option<Vec<String>>,
    pub(crate) title_sorts_contains: Option<Vec<String>>,
    pub(crate) title_sorts_contains_excluded: Option<Vec<String>>,
    pub(crate) title_sorts_begins_with: Option<Vec<String>>,
    pub(crate) title_sorts_begins_with_excluded: Option<Vec<String>>,
    pub(crate) title_sorts_ends_with: Option<Vec<String>>,
    pub(crate) title_sorts_ends_with_excluded: Option<Vec<String>>,
    pub(crate) deleted: Option<bool>,
    pub(crate) oneshot: Option<bool>,
    pub(crate) exclude_newly_added: bool,
    pub(crate) read_statuses: Option<Vec<String>>,
    pub(crate) read_statuses_excluded: Option<Vec<String>>,
    pub(crate) genres: Option<Vec<String>>,
    pub(crate) genres_excluded: Option<Vec<String>>,
    pub(crate) genres_null: Option<bool>,
    pub(crate) languages: Option<Vec<String>>,
    pub(crate) languages_excluded: Option<Vec<String>>,
    pub(crate) publishers: Option<Vec<String>>,
    pub(crate) publishers_excluded: Option<Vec<String>>,
    pub(crate) age_ratings: Option<Vec<u16>>,
    pub(crate) age_ratings_excluded: Option<Vec<u16>>,
    pub(crate) age_ratings_null: Option<bool>,
    pub(crate) age_rating_gt: Option<u16>,
    pub(crate) age_rating_lt: Option<u16>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) tags_excluded: Option<Vec<String>>,
    pub(crate) tags_null: Option<bool>,
    pub(crate) release_dates: Option<Vec<String>>,
    pub(crate) release_dates_excluded: Option<Vec<String>>,
    pub(crate) release_dates_null: Option<bool>,
    pub(crate) release_date_gt: Option<String>,
    pub(crate) release_date_lt: Option<String>,
    pub(crate) release_date_begins_with: Option<Vec<String>>,
    pub(crate) release_date_ends_with: Option<Vec<String>>,
    pub(crate) release_date_contains_excluded: Option<Vec<String>>,
    pub(crate) release_date_begins_with_excluded: Option<Vec<String>>,
    pub(crate) release_date_ends_with_excluded: Option<Vec<String>>,
    pub(crate) release_date_in_last_days: Option<i64>,
    pub(crate) release_date_not_in_last_days: Option<i64>,
    pub(crate) sharing_labels: Option<Vec<String>>,
    pub(crate) sharing_labels_excluded: Option<Vec<String>>,
    pub(crate) sharing_labels_null: Option<bool>,
    pub(crate) series_statuses: Option<Vec<String>>,
    pub(crate) series_statuses_excluded: Option<Vec<String>>,
    pub(crate) complete: Option<bool>,
    pub(crate) authors: Option<Vec<String>>,
    pub(crate) authors_excluded: Option<Vec<String>>,
}

#[derive(Clone)]
pub(crate) struct PersistedSeriesBrowseQuery {
    pub(crate) filters: SeriesFilterCriteria,
    pub(crate) search: Option<String>,
    pub(crate) page: usize,
    pub(crate) size: usize,
    pub(crate) unpaged: bool,
    pub(crate) sort_modes: Vec<PersistedSeriesSortMode>,
}

impl PersistedSeriesBrowseQuery {
    pub(crate) fn from_filters(
        filters: SeriesFilterCriteria,
        search: Option<String>,
        page: usize,
        size: usize,
        unpaged: bool,
        sort_modes: Vec<PersistedSeriesSortMode>,
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

    pub(crate) fn from_runtime_filters(
        filters: &RuntimeSeriesFilters,
        search: Option<String>,
        page: usize,
        size: usize,
        unpaged: bool,
        sort_modes: Vec<PersistedSeriesSortMode>,
    ) -> Self {
        Self::from_filters(
            filters.criteria.clone(),
            search,
            page,
            size,
            unpaged,
            sort_modes,
        )
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
pub(crate) enum PersistedSeriesSortMode {
    TitleAsc,
    CreatedDesc,
    LastModifiedDesc,
    ReleaseDateDesc,
    BooksCountDesc,
    RelevanceAsc,
    RelevanceDesc,
}

#[derive(Clone)]
pub struct PersistedSeriesSummary {
    pub id: String,
    pub library_id: String,
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
pub(crate) struct RuntimeSeriesFilters {
    pub(crate) criteria: SeriesFilterCriteria,
}

impl RuntimeSeriesFilters {
    pub(crate) fn from_criteria(criteria: SeriesFilterCriteria) -> Self {
        Self { criteria }
    }

    pub(crate) fn into_criteria(self) -> SeriesFilterCriteria {
        self.criteria
    }
}

impl Deref for RuntimeSeriesFilters {
    type Target = SeriesFilterCriteria;

    fn deref(&self) -> &Self::Target {
        &self.criteria
    }
}

impl DerefMut for RuntimeSeriesFilters {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.criteria
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BooksFilterCriteria {
    pub(crate) library_ids: Option<Vec<String>>,
    pub(crate) series_ids: Option<Vec<String>>,
    pub(crate) series_ids_excluded: Option<Vec<String>>,
    pub(crate) read_list_ids: Option<Vec<String>>,
    pub(crate) read_list_ids_excluded: Option<Vec<String>>,
    pub(crate) titles: Option<Vec<String>>,
    pub(crate) titles_excluded: Option<Vec<String>>,
    pub(crate) titles_contains: Option<Vec<String>>,
    pub(crate) titles_contains_excluded: Option<Vec<String>>,
    pub(crate) titles_begins_with: Option<Vec<String>>,
    pub(crate) titles_begins_with_excluded: Option<Vec<String>>,
    pub(crate) titles_ends_with: Option<Vec<String>>,
    pub(crate) titles_ends_with_excluded: Option<Vec<String>>,
    pub(crate) deleted: Option<bool>,
    pub(crate) oneshot: Option<bool>,
    pub(crate) genres: Option<Vec<String>>,
    pub(crate) genres_excluded: Option<Vec<String>>,
    pub(crate) genres_null: Option<bool>,
    pub(crate) languages: Option<Vec<String>>,
    pub(crate) languages_excluded: Option<Vec<String>>,
    pub(crate) publishers: Option<Vec<String>>,
    pub(crate) publishers_excluded: Option<Vec<String>>,
    pub(crate) age_ratings: Option<Vec<u16>>,
    pub(crate) age_ratings_excluded: Option<Vec<u16>>,
    pub(crate) age_ratings_null: Option<bool>,
    pub(crate) age_rating_gt: Option<u16>,
    pub(crate) age_rating_lt: Option<u16>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) tags_excluded: Option<Vec<String>>,
    pub(crate) tags_null: Option<bool>,
    pub(crate) read_statuses: Option<Vec<String>>,
    pub(crate) read_statuses_excluded: Option<Vec<String>>,
    pub(crate) media_profiles: Option<Vec<String>>,
    pub(crate) media_profiles_excluded: Option<Vec<String>>,
    pub(crate) media_statuses: Option<Vec<String>>,
    pub(crate) media_statuses_excluded: Option<Vec<String>>,
    pub(crate) authors: Option<Vec<String>>,
    pub(crate) authors_excluded: Option<Vec<String>>,
    pub(crate) poster_types: Option<Vec<String>>,
    pub(crate) poster_types_excluded: Option<Vec<String>>,
    pub(crate) poster_selected: Option<bool>,
    pub(crate) poster_selected_excluded: Option<bool>,
    pub(crate) release_dates: Option<Vec<String>>,
    pub(crate) release_dates_excluded: Option<Vec<String>>,
    pub(crate) release_dates_null: Option<bool>,
    pub(crate) release_date_gt: Option<String>,
    pub(crate) release_date_lt: Option<String>,
    pub(crate) release_date_begins_with: Option<Vec<String>>,
    pub(crate) release_date_ends_with: Option<Vec<String>>,
    pub(crate) release_date_contains_excluded: Option<Vec<String>>,
    pub(crate) release_date_begins_with_excluded: Option<Vec<String>>,
    pub(crate) release_date_ends_with_excluded: Option<Vec<String>>,
    pub(crate) release_date_in_last_days: Option<i64>,
    pub(crate) release_date_not_in_last_days: Option<i64>,
    pub(crate) number_sorts: Option<Vec<f64>>,
    pub(crate) number_sorts_excluded: Option<Vec<f64>>,
    pub(crate) number_sort_gt: Option<f64>,
    pub(crate) number_sort_lt: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RuntimeBooksFilters {
    pub(crate) direct_browse_family: Option<DirectBrowseBooksListFamily>,
    pub(crate) criteria: BooksFilterCriteria,
}

impl RuntimeBooksFilters {
    pub(crate) fn from_criteria(criteria: BooksFilterCriteria) -> Self {
        Self {
            direct_browse_family: None,
            criteria,
        }
    }
}

impl Deref for RuntimeBooksFilters {
    type Target = BooksFilterCriteria;

    fn deref(&self) -> &Self::Target {
        &self.criteria
    }
}

impl DerefMut for RuntimeBooksFilters {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.criteria
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistedBooksSortMode {
    TitleAsc,
    CreatedDateDesc,
    LastModifiedDateDesc,
    ReleaseDateDesc,
    NumberSortAsc,
    SeriesIdAsc,
    RelevanceAsc,
    RelevanceDesc,
}

#[derive(Clone)]
pub(crate) struct PersistedBooksBrowseQuery {
    pub(crate) filters: BooksFilterCriteria,
    pub(crate) search: Option<String>,
    pub(crate) page: usize,
    pub(crate) size: usize,
    pub(crate) unpaged: bool,
    pub(crate) sort_modes: Vec<PersistedBooksSortMode>,
}

impl PersistedBooksBrowseQuery {
    pub(crate) fn from_filters(
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

    pub(crate) fn from_runtime_filters(
        filters: &RuntimeBooksFilters,
        search: Option<String>,
        page: usize,
        size: usize,
        unpaged: bool,
        sort_modes: Vec<PersistedBooksSortMode>,
    ) -> Self {
        Self::from_filters(
            filters.criteria.clone(),
            search,
            page,
            size,
            unpaged,
            sort_modes,
        )
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
    pub title: String,
    pub created: String,
    pub last_modified: String,
    pub media_status: String,
    pub media_type: String,
    pub read_status: String,
    pub metadata_number_sort: Option<f64>,
    pub metadata_release_date: Option<String>,
    pub deleted: bool,
    pub oneshot: bool,
    pub genres: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub age_rating: Option<u16>,
    pub metadata_tags: Vec<String>,
    pub metadata_authors: Vec<String>,
}

#[derive(Clone)]
pub struct PersistedBookPosterSummary {
    pub thumbnail_type: String,
    pub selected: bool,
}
