use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::discovery::{
    BookReadModel, BookTagScope, BooksBrowseRequest, DiscoveryBrowseService, DiscoveryFacetService,
    FacetKind, FacetScope, LatestBooksRequest, SeriesAlphabeticalGroupsRequest,
    SeriesBrowseRequest, SeriesReadModel,
};
use komga_domain::discovery::{
    AgeRestrictionKind, BookSort, DiscoveryError,
    DiscoveryQueryContext as DomainDiscoveryQueryContext, PageEnvelope, SeriesSort,
};
use serde_json::{Value, json};

use crate::database_handle::DatabaseHandle;
use crate::discovery_persisted_access::{
    books, facets, library_mappings, models as persisted_models, runtime_queries, series,
};
use crate::search::index_lifecycle::{SearchEntityType, SearchQueryLifecycle};

mod books_queries;
mod common_helpers;
mod models;
mod series_queries;

use models::{
    BooksFilterCriteria, PersistedBookPosterSummary, PersistedBookSummary,
    PersistedBooksBrowseQuery, PersistedBooksSortMode, PersistedReadProgressSummary,
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, PersistedSeriesSummary,
    PersistedWebLinkEntry, SeriesFilterCriteria,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueryRestrictions {
    age: Option<u16>,
    age_restriction: Option<AgeRestrictionKind>,
    labels_allow: Vec<String>,
    labels_exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiscoveryQueryContext {
    user_id: Option<String>,
    is_admin: bool,
    authorized_library_ids: Option<Vec<String>>,
    restrictions: Option<QueryRestrictions>,
}

#[async_trait]
trait PersistedDiscoveryBrowseDataSource: Send + Sync {
    async fn load_book_poster_summaries(
        &self,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String>;

    async fn load_persisted_book_summaries(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<PersistedBookSummary>, String>;

    async fn load_persisted_book_summaries_by_ids(
        &self,
        user_id: Option<&str>,
        ids: &[String],
    ) -> Result<Vec<PersistedBookSummary>, String>;

    async fn load_persisted_book_count(&self) -> Result<usize, String>;

    async fn load_collection_memberships(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String>;

    async fn load_collection_ordering(
        &self,
        collection_id: &str,
    ) -> Result<HashMap<String, i64>, String>;

    async fn load_readlist_memberships(&self)
    -> Result<BTreeMap<String, BTreeSet<String>>, String>;

    async fn load_readlist_ordering(
        &self,
        readlist_id: &str,
    ) -> Result<HashMap<String, i64>, String>;

    async fn persisted_utc_date_minus_days(&self, days: i64) -> Result<Option<String>, String>;

    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String>;

    async fn load_series_read_dates(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, String>, String>;

    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String>;

    async fn load_persisted_series_summaries(&self) -> Result<Vec<PersistedSeriesSummary>, String>;

    async fn load_persisted_series_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<PersistedSeriesSummary>, String>;

    async fn load_persisted_series_count(&self) -> Result<usize, String>;

    async fn search_book_ids(&self, query: &str, limit: usize) -> Result<Vec<String>, String>;

    async fn search_series_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;
}

#[derive(Clone)]
pub struct SqliteDiscoveryBrowseService {
    db: DatabaseHandle,
    index_dir: PathBuf,
}

impl SqliteDiscoveryBrowseService {
    pub fn new(db: DatabaseHandle, index_dir: PathBuf) -> Self {
        Self { db, index_dir }
    }
}

fn search_ids_or_empty(
    index_dir: &std::path::Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<String> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_ids(query, entity_type, limit)
        .unwrap_or_default()
}

fn search_scored_ids_or_empty(
    index_dir: &std::path::Path,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Vec<(f32, String)> {
    let Ok(index) = SearchQueryLifecycle::bootstrap(index_dir) else {
        return Vec::new();
    };

    index
        .search_scored_ids(query, entity_type, limit)
        .unwrap_or_default()
}

fn to_browse_context(context: &DomainDiscoveryQueryContext) -> DiscoveryQueryContext {
    DiscoveryQueryContext {
        user_id: context.user_id.as_ref().map(|id| id.as_str().to_string()),
        is_admin: context.is_admin,
        authorized_library_ids: context
            .authorized_library_ids
            .as_ref()
            .map(|ids| ids.iter().map(|id| id.as_str().to_string()).collect()),
        restrictions: context
            .restrictions
            .as_ref()
            .map(|restrictions| QueryRestrictions {
                age: restrictions.age,
                age_restriction: restrictions.age_restriction,
                labels_allow: restrictions.labels_allow.clone(),
                labels_exclude: restrictions.labels_exclude.clone(),
            }),
    }
}

fn series_sort_to_persisted(sort: &[SeriesSort]) -> Vec<PersistedSeriesSortMode> {
    sort.iter()
        .map(|sort| match sort {
            SeriesSort::MetadataTitleSortAsc => PersistedSeriesSortMode::TitleAsc,
            SeriesSort::MetadataTitleSortDesc => PersistedSeriesSortMode::TitleDesc,
            SeriesSort::NameAsc => PersistedSeriesSortMode::NameAsc,
            SeriesSort::NameDesc => PersistedSeriesSortMode::NameDesc,
            SeriesSort::CreatedDateAsc => PersistedSeriesSortMode::CreatedAsc,
            SeriesSort::CreatedDateDesc => PersistedSeriesSortMode::CreatedDesc,
            SeriesSort::LastModifiedDateAsc => PersistedSeriesSortMode::LastModifiedAsc,
            SeriesSort::LastModifiedDateDesc => PersistedSeriesSortMode::LastModifiedDesc,
            SeriesSort::ReleaseDateAsc => PersistedSeriesSortMode::ReleaseDateAsc,
            SeriesSort::ReleaseDateDesc => PersistedSeriesSortMode::ReleaseDateDesc,
            SeriesSort::BooksCountAsc => PersistedSeriesSortMode::BooksCountAsc,
            SeriesSort::BooksCountDesc => PersistedSeriesSortMode::BooksCountDesc,
            SeriesSort::CollectionNumberAsc => PersistedSeriesSortMode::CollectionNumberAsc,
            SeriesSort::CollectionNumberDesc => PersistedSeriesSortMode::CollectionNumberDesc,
            SeriesSort::ReadDateAsc => PersistedSeriesSortMode::ReadDateAsc,
            SeriesSort::ReadDateDesc => PersistedSeriesSortMode::ReadDateDesc,
            SeriesSort::Random => PersistedSeriesSortMode::Random,
            SeriesSort::RelevanceAsc => PersistedSeriesSortMode::RelevanceAsc,
            SeriesSort::RelevanceDesc => PersistedSeriesSortMode::RelevanceDesc,
        })
        .collect()
}

fn book_sort_to_persisted(sort: &[BookSort]) -> Vec<PersistedBooksSortMode> {
    sort.iter()
        .filter_map(|sort| match sort {
            BookSort::MetadataTitleAsc => Some(PersistedBooksSortMode::TitleAsc),
            BookSort::MetadataTitleDesc => Some(PersistedBooksSortMode::TitleDesc),
            BookSort::NameAsc => Some(PersistedBooksSortMode::NameAsc),
            BookSort::NameDesc => Some(PersistedBooksSortMode::NameDesc),
            BookSort::SeriesTitleAsc => Some(PersistedBooksSortMode::SeriesTitleAsc),
            BookSort::SeriesTitleDesc => Some(PersistedBooksSortMode::SeriesTitleDesc),
            BookSort::CreatedDateAsc => Some(PersistedBooksSortMode::CreatedDateAsc),
            BookSort::CreatedDateDesc => Some(PersistedBooksSortMode::CreatedDateDesc),
            BookSort::LastModifiedDateAsc => Some(PersistedBooksSortMode::LastModifiedDateAsc),
            BookSort::LastModifiedDateDesc => Some(PersistedBooksSortMode::LastModifiedDateDesc),
            BookSort::FileSizeAsc => Some(PersistedBooksSortMode::FileSizeAsc),
            BookSort::FileSizeDesc => Some(PersistedBooksSortMode::FileSizeDesc),
            BookSort::FileHashAsc => Some(PersistedBooksSortMode::FileHashAsc),
            BookSort::FileHashDesc => Some(PersistedBooksSortMode::FileHashDesc),
            BookSort::UrlAsc => Some(PersistedBooksSortMode::UrlAsc),
            BookSort::UrlDesc => Some(PersistedBooksSortMode::UrlDesc),
            BookSort::MediaStatusAsc => Some(PersistedBooksSortMode::MediaStatusAsc),
            BookSort::MediaStatusDesc => Some(PersistedBooksSortMode::MediaStatusDesc),
            BookSort::MediaCommentAsc => Some(PersistedBooksSortMode::MediaCommentAsc),
            BookSort::MediaCommentDesc => Some(PersistedBooksSortMode::MediaCommentDesc),
            BookSort::MediaTypeAsc => Some(PersistedBooksSortMode::MediaTypeAsc),
            BookSort::MediaTypeDesc => Some(PersistedBooksSortMode::MediaTypeDesc),
            BookSort::MediaPagesCountAsc => Some(PersistedBooksSortMode::MediaPagesCountAsc),
            BookSort::MediaPagesCountDesc => Some(PersistedBooksSortMode::MediaPagesCountDesc),
            BookSort::ReadProgressLastModifiedAsc => {
                Some(PersistedBooksSortMode::ReadProgressLastModifiedDateAsc)
            }
            BookSort::ReadProgressLastModifiedDesc => {
                Some(PersistedBooksSortMode::ReadProgressLastModifiedDateDesc)
            }
            BookSort::ReadProgressReadDateAsc => {
                Some(PersistedBooksSortMode::ReadProgressReadDateAsc)
            }
            BookSort::ReadProgressReadDateDesc => {
                Some(PersistedBooksSortMode::ReadProgressReadDateDesc)
            }
            BookSort::ReleaseDateAsc => Some(PersistedBooksSortMode::ReleaseDateAsc),
            BookSort::ReleaseDateDesc => Some(PersistedBooksSortMode::ReleaseDateDesc),
            BookSort::NumberSortAsc => Some(PersistedBooksSortMode::NumberSortAsc),
            BookSort::NumberSortDesc => Some(PersistedBooksSortMode::NumberSortDesc),
            BookSort::SeriesIdAsc => Some(PersistedBooksSortMode::SeriesIdAsc),
            BookSort::ReadListNumberAsc => Some(PersistedBooksSortMode::ReadListNumberAsc),
            BookSort::ReadListNumberDesc => Some(PersistedBooksSortMode::ReadListNumberDesc),
            BookSort::RelevanceAsc => Some(PersistedBooksSortMode::RelevanceAsc),
            BookSort::RelevanceDesc => Some(PersistedBooksSortMode::RelevanceDesc),
            BookSort::Random => None,
        })
        .collect()
}

fn persisted_series_to_read_model(series: &PersistedSeriesSummary) -> SeriesReadModel {
    SeriesReadModel {
        id: series.id.clone(),
        library_id: series.library_id.clone(),
        name: series.name.clone(),
        title: series.title.clone(),
        title_sort: series.title_sort.clone(),
        labels: series.labels.clone(),
        created: series.created.clone(),
        last_modified: series.last_modified.clone(),
        file_last_modified: series.file_last_modified.clone(),
        books_count: series.books_count,
        books_read_count: series.books_read_count,
        books_unread_count: series.books_unread_count,
        books_in_progress_count: series.books_in_progress_count,
        status: series.status.clone(),
        summary: series.summary.clone(),
        reading_direction: series.reading_direction.clone(),
        publisher: series.publisher.clone(),
        age_rating: series.age_rating,
        language: series.language.clone(),
        genres: series.genres.clone(),
        tags: series.tags.clone(),
        alternate_titles: series.alternate_titles.clone(),
        metadata_created: series.metadata_created.clone(),
        metadata_last_modified: series.metadata_last_modified.clone(),
        books_metadata_authors: series.books_metadata_authors.clone(),
        books_metadata_tags: series.books_metadata_tags.clone(),
        books_metadata_release_date: series.books_metadata_release_date.clone(),
        books_metadata_summary: series.books_metadata_summary.clone(),
        books_metadata_summary_number: series.books_metadata_summary_number.clone(),
        books_metadata_created: series.books_metadata_created.clone(),
        books_metadata_last_modified: series.books_metadata_last_modified.clone(),
        deleted: series.deleted,
        oneshot: series.oneshot,
    }
}

fn map_series_page(page: PageEnvelope<PersistedSeriesSummary>) -> PageEnvelope<SeriesReadModel> {
    PageEnvelope {
        content: page
            .content
            .iter()
            .map(persisted_series_to_read_model)
            .collect(),
        page: page.page,
        size: page.size,
        total_elements: page.total_elements,
        total_pages: page.total_pages,
    }
}

fn persisted_book_summary(row: persisted_models::BookSummary) -> PersistedBookSummary {
    PersistedBookSummary {
        id: row.id,
        series_id: row.series_id,
        library_id: row.library_id,
        series_title: row.series_title,
        series_title_sort: row.series_title_sort,
        title: row.title,
        name: row.name,
        url: row.url,
        number: row.number,
        created: row.created,
        last_modified: row.last_modified,
        file_last_modified: row.file_last_modified,
        size_bytes: row.size_bytes,
        media_status: row.media_status,
        media_type: row.media_type,
        media_pages_count: row.media_pages_count,
        media_comment: row.media_comment,
        media_epub_divina_compatible: row.media_epub_divina_compatible,
        media_epub_is_kepub: row.media_epub_is_kepub,
        read_status: row.read_status,
        metadata_title_lock: row.metadata_title_lock,
        metadata_summary: row.metadata_summary,
        metadata_summary_lock: row.metadata_summary_lock,
        metadata_number: row.metadata_number,
        metadata_number_lock: row.metadata_number_lock,
        metadata_number_sort: row.metadata_number_sort,
        metadata_number_sort_lock: row.metadata_number_sort_lock,
        metadata_release_date: row.metadata_release_date,
        metadata_release_date_lock: row.metadata_release_date_lock,
        metadata_authors_lock: row.metadata_authors_lock,
        metadata_tags_lock: row.metadata_tags_lock,
        metadata_isbn: row.metadata_isbn,
        metadata_isbn_lock: row.metadata_isbn_lock,
        metadata_links_lock: row.metadata_links_lock,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        file_hash: row.file_hash,
        read_progress: row
            .read_progress
            .map(|progress| PersistedReadProgressSummary {
                page: progress.page,
                completed: progress.completed,
                read_date: progress.read_date,
                created: progress.created,
                last_modified: progress.last_modified,
                device_id: progress.device_id,
                device_name: progress.device_name,
            }),
        deleted: row.deleted,
        oneshot: row.oneshot,
        genres: row.genres,
        language: row.language,
        publisher: row.publisher,
        age_rating: row.age_rating,
        metadata_tags: row.metadata_tags,
        metadata_authors: row
            .metadata_authors
            .into_iter()
            .map(|author| models::PersistedAuthorEntry {
                name: author.name,
                role: author.role,
            })
            .collect(),
        metadata_links: row
            .metadata_links
            .into_iter()
            .map(|link| PersistedWebLinkEntry {
                label: link.label,
                url: link.url,
            })
            .collect(),
    }
}

fn persisted_series_summary(row: persisted_models::SeriesSummary) -> PersistedSeriesSummary {
    PersistedSeriesSummary {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
        title_sort: row.title_sort,
        labels: row.labels,
        created: row.created,
        last_modified: row.last_modified,
        file_last_modified: row.file_last_modified,
        books_count: row.books_count,
        books_read_count: row.books_read_count,
        books_unread_count: row.books_unread_count,
        books_in_progress_count: row.books_in_progress_count,
        status: row.status,
        summary: row.summary,
        reading_direction: row.reading_direction,
        publisher: row.publisher,
        age_rating: row.age_rating,
        language: row.language,
        genres: row.genres,
        tags: row.tags,
        alternate_titles: row.alternate_titles,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        books_metadata_authors: row.books_metadata_authors,
        books_metadata_tags: row.books_metadata_tags,
        books_metadata_release_date: row.books_metadata_release_date,
        books_metadata_summary: row.books_metadata_summary,
        books_metadata_summary_number: row.books_metadata_summary_number,
        books_metadata_created: row.books_metadata_created,
        books_metadata_last_modified: row.books_metadata_last_modified,
        deleted: row.deleted,
        oneshot: row.oneshot,
    }
}

#[async_trait]
impl PersistedDiscoveryBrowseDataSource for SqliteDiscoveryBrowseService {
    async fn load_book_poster_summaries(
        &self,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
        let rows = books::load_book_poster_summaries(self.db.read_pool()).await?;
        Ok(rows
            .into_iter()
            .map(|(book_id, values)| {
                (
                    book_id,
                    values
                        .into_iter()
                        .map(|value| PersistedBookPosterSummary {
                            thumbnail_type: value.thumbnail_type,
                            selected: value.selected,
                        })
                        .collect(),
                )
            })
            .collect())
    }

    async fn load_persisted_book_summaries(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        books::load_persisted_book_summaries(self.db.read_pool(), user_id)
            .await
            .map(|rows| rows.into_iter().map(persisted_book_summary).collect())
    }

    async fn load_persisted_book_summaries_by_ids(
        &self,
        user_id: Option<&str>,
        ids: &[String],
    ) -> Result<Vec<PersistedBookSummary>, String> {
        books::load_persisted_book_summaries_by_ids(self.db.read_pool(), user_id, ids)
            .await
            .map(|rows| rows.into_iter().map(persisted_book_summary).collect())
    }

    async fn load_persisted_book_count(&self) -> Result<usize, String> {
        books::load_persisted_book_count(self.db.read_pool()).await
    }

    async fn load_collection_memberships(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        library_mappings::load_collection_memberships(self.db.read_pool()).await
    }

    async fn load_collection_ordering(
        &self,
        collection_id: &str,
    ) -> Result<HashMap<String, i64>, String> {
        library_mappings::load_collection_ordering(self.db.read_pool(), collection_id).await
    }

    async fn load_readlist_memberships(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        library_mappings::load_readlist_memberships(self.db.read_pool()).await
    }

    async fn load_readlist_ordering(
        &self,
        readlist_id: &str,
    ) -> Result<HashMap<String, i64>, String> {
        library_mappings::load_readlist_ordering(self.db.read_pool(), readlist_id).await
    }

    async fn persisted_utc_date_minus_days(&self, days: i64) -> Result<Option<String>, String> {
        runtime_queries::persisted_utc_date_minus_days(self.db.read_pool(), days).await
    }

    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        runtime_queries::load_series_read_progress_counts(self.db.read_pool(), user_id).await
    }

    async fn load_series_read_dates(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, String>, String> {
        runtime_queries::load_series_read_dates(self.db.read_pool(), user_id).await
    }

    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String> {
        runtime_queries::load_series_total_book_counts(self.db.read_pool()).await
    }

    async fn load_persisted_series_summaries(&self) -> Result<Vec<PersistedSeriesSummary>, String> {
        series::load_persisted_series_summaries(self.db.read_pool())
            .await
            .map(|rows| rows.into_iter().map(persisted_series_summary).collect())
    }

    async fn load_persisted_series_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        series::load_persisted_series_summaries_by_ids(self.db.read_pool(), ids)
            .await
            .map(|rows| rows.into_iter().map(persisted_series_summary).collect())
    }

    async fn load_persisted_series_count(&self) -> Result<usize, String> {
        series::load_persisted_series_count(self.db.read_pool()).await
    }

    async fn search_book_ids(&self, query: &str, limit: usize) -> Result<Vec<String>, String> {
        Ok(search_ids_or_empty(
            self.index_dir.as_path(),
            query,
            SearchEntityType::Book,
            limit,
        ))
    }

    async fn search_series_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        Ok(search_scored_ids_or_empty(
            self.index_dir.as_path(),
            query,
            SearchEntityType::Series,
            limit,
        ))
    }
}

#[async_trait]
impl DiscoveryBrowseService for SqliteDiscoveryBrowseService {
    async fn list_series(
        &self,
        context: &DomainDiscoveryQueryContext,
        request: SeriesBrowseRequest,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        let context = to_browse_context(context);
        let persisted_query = PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria::default(),
            request.search,
            request.page.page,
            request.page.size,
            request.page.unpaged,
            series_sort_to_persisted(&request.sort),
        )
        .with_condition(request.filter.condition);

        let page = series_queries::load_persisted_series_page(self, &context, persisted_query)
            .await
            .map_err(DiscoveryError::Persistence)?;
        Ok(map_series_page(page))
    }

    async fn list_books(
        &self,
        context: &DomainDiscoveryQueryContext,
        request: BooksBrowseRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let context = to_browse_context(context);
        let persisted_query = PersistedBooksBrowseQuery::from_filters(
            BooksFilterCriteria::default(),
            request.search,
            request.page.page,
            request.page.size,
            request.page.unpaged,
            book_sort_to_persisted(&request.sort),
        )
        .with_condition(request.filter.condition);

        books_queries::load_persisted_books_page(self, &context, persisted_query)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_latest_books(
        &self,
        context: &DomainDiscoveryQueryContext,
        request: LatestBooksRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let context = to_browse_context(context);
        let persisted_query = PersistedBooksBrowseQuery::from_filters(
            BooksFilterCriteria {
                library_ids: request.library_ids,
                ..BooksFilterCriteria::default()
            },
            None,
            request.page.page,
            request.page.size,
            request.page.unpaged,
            vec![PersistedBooksSortMode::LastModifiedDateDesc],
        );

        books_queries::load_persisted_books_page(self, &context, persisted_query)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_series_alphabetical_groups(
        &self,
        context: &DomainDiscoveryQueryContext,
        request: SeriesAlphabeticalGroupsRequest,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError> {
        let context = to_browse_context(context);
        series_queries::load_persisted_alphabetical_groups(
            self,
            &context,
            request.filter.condition,
            request.search,
        )
        .await
        .map_err(DiscoveryError::Persistence)
    }
}

#[async_trait]
impl DiscoveryFacetService for SqliteDiscoveryBrowseService {
    async fn list_facet_values(
        &self,
        _context: &DomainDiscoveryQueryContext,
        kind: FacetKind,
        scope: FacetScope,
    ) -> Result<Vec<String>, DiscoveryError> {
        let db = self.db.read_pool();
        let library_ids = scope.library_ids.as_deref();
        let collection_id = scope.collection_id.as_deref();

        match kind {
            FacetKind::Genres => {
                facets::load_persisted_genres(db, library_ids, collection_id).await
            }
            FacetKind::Tags => facets::load_persisted_tags(db, library_ids, collection_id).await,
            FacetKind::Languages => {
                facets::load_persisted_languages(db, library_ids, collection_id).await
            }
            FacetKind::Publishers => {
                facets::load_persisted_publishers(db, library_ids, collection_id).await
            }
            FacetKind::AgeRatings => {
                facets::load_persisted_age_ratings(db, library_ids, collection_id).await
            }
            FacetKind::SharingLabels => {
                facets::load_persisted_sharing_labels(db, library_ids, collection_id).await
            }
            FacetKind::SeriesTags => {
                facets::load_persisted_series_tags(db, library_ids, collection_id).await
            }
            FacetKind::SeriesReleaseDates => {
                facets::load_persisted_series_release_dates(db, library_ids, collection_id).await
            }
        }
        .map_err(DiscoveryError::Persistence)
    }

    async fn list_book_tags(
        &self,
        _context: &DomainDiscoveryQueryContext,
        scope: Option<BookTagScope>,
        library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, DiscoveryError> {
        let scope = scope.map(|scope| match scope {
            BookTagScope::All => persisted_models::BookTagsScope::All,
            BookTagScope::Series(id) => persisted_models::BookTagsScope::Series(id),
            BookTagScope::Libraries(ids) => persisted_models::BookTagsScope::Libraries(ids),
            BookTagScope::ReadList(id) => persisted_models::BookTagsScope::ReadList(id),
        });

        runtime_queries::load_persisted_book_tags(
            self.db.read_pool(),
            scope.as_ref(),
            library_ids.as_deref(),
        )
        .await
        .map_err(DiscoveryError::Persistence)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use komga_application::discovery::{
        DiscoveryBrowseService, DiscoveryFacetService, PageRequest,
    };
    use komga_domain::common_ids::{LibraryId, UserId};
    use komga_domain::discovery::{
        BookCondition, BookFilter, BookValueCondition, CompositeBookCondition,
        CompositeSeriesCondition, DiscoveryQueryContext, FilterOperator, InclusionCondition,
        QueryRestrictions, SeriesCondition, SeriesFilter, SeriesValueCondition, StringCondition,
    };
    use serde_json::json;
    use sqlx::SqlitePool;

    use super::*;
    use crate::sqlite::{connect_main_write_context, evict_shared_pools_for_paths};

    struct BrowseFixture {
        service: SqliteDiscoveryBrowseService,
        pool: SqlitePool,
        database_file: PathBuf,
    }

    impl BrowseFixture {
        async fn new(case: &str) -> Self {
            let database_file = unique_database_path(case);
            let context = connect_main_write_context(database_file.as_path())
                .await
                .expect("fixture database should bootstrap");
            let pool = context.pool().clone();
            let db = DatabaseHandle::file_backed(database_file.clone())
                .await
                .expect("fixture database handle should open");
            let service = SqliteDiscoveryBrowseService::new(
                db,
                database_file.with_extension("tantivy-index"),
            );
            Self {
                service,
                pool,
                database_file,
            }
        }

        async fn cleanup(self) {
            let pools = evict_shared_pools_for_paths(std::slice::from_ref(&self.database_file));
            drop(self.service);
            self.pool.close().await;
            for pool in pools {
                pool.close().await;
            }
            let _ = std::fs::remove_file(self.database_file);
        }
    }

    fn unique_database_path(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-discovery-browse-{case}-{}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    fn unrestricted_context() -> DiscoveryQueryContext {
        DiscoveryQueryContext {
            user_id: None,
            is_admin: true,
            authorized_library_ids: None,
            restrictions: None,
        }
    }

    fn restricted_context(
        authorized_library_ids: Vec<&str>,
        restrictions: QueryRestrictions,
    ) -> DiscoveryQueryContext {
        DiscoveryQueryContext {
            user_id: Some(UserId::from("user-1")),
            is_admin: false,
            authorized_library_ids: Some(
                authorized_library_ids
                    .into_iter()
                    .map(LibraryId::from)
                    .collect(),
            ),
            restrictions: Some(restrictions),
        }
    }

    fn exclude_age(age: u16) -> QueryRestrictions {
        QueryRestrictions {
            age: Some(age),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec![],
        }
    }

    fn allow_labels(labels: &[&str]) -> QueryRestrictions {
        QueryRestrictions {
            age: None,
            age_restriction: None,
            labels_allow: labels.iter().map(|label| label.to_string()).collect(),
            labels_exclude: vec![],
        }
    }

    async fn insert_library(pool: &SqlitePool, id: &str) {
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind(id)
            .bind(id)
            .bind(format!("/tmp/{id}"))
            .execute(pool)
            .await
            .expect("library row should be inserted");
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_series(
        pool: &SqlitePool,
        id: &str,
        library_id: &str,
        title: &str,
        age_rating: Option<i64>,
        labels: &[&str],
        genres: &[&str],
        tags: &[&str],
    ) {
        sqlx::query(
            r#"INSERT INTO SERIES (
                ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, BOOK_COUNT
            ) VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(0_i64)
        .bind(title)
        .bind(format!("series/{id}"))
        .bind(library_id)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("series row should be inserted");

        sqlx::query(
            r#"INSERT INTO SERIES_METADATA (
                STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID
            ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("ONGOING")
        .bind(title)
        .bind(title)
        .bind("Publisher")
        .bind("en")
        .bind(age_rating)
        .bind(id)
        .execute(pool)
        .await
        .expect("series metadata row should be inserted");

        for label in labels {
            sqlx::query("INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) VALUES (?, ?)")
                .bind(id)
                .bind(label)
                .execute(pool)
                .await
                .expect("series sharing label should be inserted");
        }
        for genre in genres {
            sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
                .bind(id)
                .bind(genre)
                .execute(pool)
                .await
                .expect("series genre should be inserted");
        }
        for tag in tags {
            sqlx::query("INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) VALUES (?, ?)")
                .bind(id)
                .bind(tag)
                .execute(pool)
                .await
                .expect("series tag should be inserted");
        }
    }

    async fn insert_book(
        pool: &SqlitePool,
        id: &str,
        series_id: &str,
        library_id: &str,
        title: &str,
        tag: &str,
        last_modified: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO BOOK (
                ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID,
                FILE_HASH, LAST_MODIFIED_DATE
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(0_i64)
        .bind(format!("{title}.cbz"))
        .bind(format!("books/{id}.cbz"))
        .bind(series_id)
        .bind(1024_i64)
        .bind(1_i64)
        .bind(library_id)
        .bind(format!("hash-{id}"))
        .bind(last_modified)
        .execute(pool)
        .await
        .expect("book row should be inserted");

        sqlx::query(
            r#"INSERT INTO BOOK_METADATA (
                NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID
            ) VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind("1")
        .bind(1.0_f64)
        .bind(title)
        .bind("2024-01-01")
        .bind(id)
        .execute(pool)
        .await
        .expect("book metadata row should be inserted");

        sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
            .bind(id)
            .bind(tag)
            .execute(pool)
            .await
            .expect("book tag should be inserted");

        sqlx::query(
            "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind("application/zip")
        .bind("READY")
        .bind(id)
        .bind(10_i64)
        .execute(pool)
        .await
        .expect("media row should be inserted");

        sqlx::query("UPDATE SERIES SET BOOK_COUNT = BOOK_COUNT + 1 WHERE ID = ?")
            .bind(series_id)
            .execute(pool)
            .await
            .expect("series book count should update");
    }

    fn tag_condition(tag: &str) -> BookCondition {
        BookCondition::Value(BookValueCondition::Tag(StringCondition::Exact(
            InclusionCondition::Include(vec![tag.to_string()]),
        )))
    }

    fn series_tag_condition(tag: &str) -> SeriesCondition {
        SeriesCondition::Value(SeriesValueCondition::Tag(StringCondition::Exact(
            InclusionCondition::Include(vec![tag.to_string()]),
        )))
    }

    fn assert_book_ids(page: PageEnvelope<BookReadModel>, expected: &[&str]) {
        assert_eq!(
            page.content
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }

    fn assert_series_ids(page: PageEnvelope<SeriesReadModel>, expected: &[&str]) {
        assert_eq!(
            page.content
                .iter()
                .map(|series| series.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[tokio::test]
    async fn list_books_filters_condition_tree_after_auth_and_age_restrictions() {
        let fixture = BrowseFixture::new("books-condition-auth-age").await;
        insert_library(&fixture.pool, "library-allowed").await;
        insert_library(&fixture.pool, "library-hidden").await;
        insert_series(
            &fixture.pool,
            "series-visible",
            "library-allowed",
            "Visible",
            Some(12),
            &[],
            &[],
            &[],
        )
        .await;
        insert_series(
            &fixture.pool,
            "series-aged-out",
            "library-allowed",
            "Aged Out",
            Some(18),
            &[],
            &[],
            &[],
        )
        .await;
        insert_series(
            &fixture.pool,
            "series-hidden",
            "library-hidden",
            "Hidden",
            Some(12),
            &[],
            &[],
            &[],
        )
        .await;
        insert_book(
            &fixture.pool,
            "book-visible",
            "series-visible",
            "library-allowed",
            "Visible Book",
            "favorite",
            "2024-01-01T00:00:00Z",
        )
        .await;
        insert_book(
            &fixture.pool,
            "book-aged-out",
            "series-aged-out",
            "library-allowed",
            "Aged Out Book",
            "queued",
            "2024-01-02T00:00:00Z",
        )
        .await;
        insert_book(
            &fixture.pool,
            "book-hidden",
            "series-hidden",
            "library-hidden",
            "Hidden Book",
            "favorite",
            "2024-01-03T00:00:00Z",
        )
        .await;

        let page = DiscoveryBrowseService::list_books(
            &fixture.service,
            &restricted_context(vec!["library-allowed"], exclude_age(16)),
            BooksBrowseRequest {
                filter: BookFilter {
                    condition: Some(BookCondition::Composite(CompositeBookCondition {
                        operator: FilterOperator::Any,
                        conditions: vec![tag_condition("favorite"), tag_condition("queued")],
                    })),
                    direct_browse_book_id: None,
                },
                ..BooksBrowseRequest::default()
            },
        )
        .await
        .expect("books browse should succeed");

        assert_book_ids(page, &["book-visible"]);
        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn list_series_and_alphabetical_groups_apply_label_restrictions() {
        let fixture = BrowseFixture::new("series-label-restrictions").await;
        insert_library(&fixture.pool, "library-1").await;
        insert_series(
            &fixture.pool,
            "series-alpha",
            "library-1",
            "Alpha",
            None,
            &["kids"],
            &[],
            &["favorite"],
        )
        .await;
        insert_series(
            &fixture.pool,
            "series-beta",
            "library-1",
            "Beta",
            None,
            &["restricted"],
            &[],
            &["queued"],
        )
        .await;

        let context = restricted_context(vec!["library-1"], allow_labels(&["kids"]));
        let page = DiscoveryBrowseService::list_series(
            &fixture.service,
            &context,
            SeriesBrowseRequest {
                filter: SeriesFilter {
                    condition: Some(SeriesCondition::Composite(CompositeSeriesCondition {
                        operator: FilterOperator::Any,
                        conditions: vec![
                            series_tag_condition("favorite"),
                            series_tag_condition("queued"),
                        ],
                    })),
                },
                ..SeriesBrowseRequest::default()
            },
        )
        .await
        .expect("series browse should succeed");
        assert_series_ids(page, &["series-alpha"]);

        let groups = DiscoveryBrowseService::list_series_alphabetical_groups(
            &fixture.service,
            &context,
            SeriesAlphabeticalGroupsRequest {
                filter: SeriesFilter { condition: None },
                search: None,
            },
        )
        .await
        .expect("alphabetical groups should load");

        assert_eq!(groups, vec![json!({ "group": "a", "count": 1 })]);
        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn list_latest_books_uses_last_modified_desc_sort() {
        let fixture = BrowseFixture::new("latest-books-sort").await;
        insert_library(&fixture.pool, "library-1").await;
        insert_series(
            &fixture.pool,
            "series-1",
            "library-1",
            "Series",
            None,
            &[],
            &[],
            &[],
        )
        .await;
        insert_book(
            &fixture.pool,
            "book-old",
            "series-1",
            "library-1",
            "Old Book",
            "old",
            "2024-01-01T00:00:00Z",
        )
        .await;
        insert_book(
            &fixture.pool,
            "book-new",
            "series-1",
            "library-1",
            "New Book",
            "new",
            "2024-02-01T00:00:00Z",
        )
        .await;

        let page = DiscoveryBrowseService::list_latest_books(
            &fixture.service,
            &unrestricted_context(),
            LatestBooksRequest {
                library_ids: None,
                page: PageRequest::default(),
            },
        )
        .await
        .expect("latest books should load");

        assert_book_ids(page, &["book-new", "book-old"]);
        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn facets_and_book_tags_read_from_persisted_sqlite() {
        let fixture = BrowseFixture::new("facets-book-tags").await;
        insert_library(&fixture.pool, "library-1").await;
        insert_library(&fixture.pool, "library-2").await;
        insert_series(
            &fixture.pool,
            "series-1",
            "library-1",
            "Series One",
            None,
            &[],
            &["Drama"],
            &["series-tag"],
        )
        .await;
        insert_series(
            &fixture.pool,
            "series-2",
            "library-2",
            "Series Two",
            None,
            &[],
            &["Action"],
            &[],
        )
        .await;
        insert_book(
            &fixture.pool,
            "book-1",
            "series-1",
            "library-1",
            "Book One",
            "book-tag",
            "2024-01-01T00:00:00Z",
        )
        .await;
        insert_book(
            &fixture.pool,
            "book-2",
            "series-2",
            "library-2",
            "Book Two",
            "hidden-book-tag",
            "2024-01-01T00:00:00Z",
        )
        .await;

        let genres = DiscoveryFacetService::list_facet_values(
            &fixture.service,
            &unrestricted_context(),
            FacetKind::Genres,
            FacetScope {
                library_ids: Some(vec!["library-1".to_string()]),
                collection_id: None,
            },
        )
        .await
        .expect("genres should load");
        assert_eq!(genres, vec!["Drama"]);

        let book_tags = DiscoveryFacetService::list_book_tags(
            &fixture.service,
            &unrestricted_context(),
            Some(BookTagScope::Series("series-1".to_string())),
            None,
        )
        .await
        .expect("book tags should load");
        assert_eq!(book_tags, vec!["book-tag"]);

        fixture.cleanup().await;
    }
}
