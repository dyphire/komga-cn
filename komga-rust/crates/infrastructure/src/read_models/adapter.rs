use async_trait::async_trait;
use komga_application::discovery::{
    BookReadModel, BookTagScope, BooksBrowseQuery, BooksBrowseRequest, BooksFeedQuery,
    DiscoveryBrowseService, DiscoveryListService, LatestBooksRequest, SeriesBrowseQuery,
    SeriesBrowseRequest, SeriesReadModel,
};
use komga_domain::discovery::{
    BookFilter, DiscoveryError, DiscoveryQueryContext, PageEnvelope, SeriesFilter,
    UnsupportedDiscoverySemantics,
};
use sqlx::SqlitePool;

use super::queries;
use super::rows::{BookRow, CollectionRow, LibraryRow, ReadListRow, ReadProgressRow, SeriesRow};
use crate::sqlite::{fixtures, setup};

pub struct SqliteDiscoveryAdapter {
    pool: SqlitePool,
}

impl SqliteDiscoveryAdapter {
    pub async fn new() -> Result<Self, DiscoveryError> {
        let pool = setup::open_in_memory_database()
            .await
            .map_err(map_sqlx_error)?;
        Ok(Self { pool })
    }

    pub async fn insert_library(&self, row: LibraryRow) -> Result<(), DiscoveryError> {
        fixtures::insert_library(&self.pool, row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn insert_series(&self, row: SeriesRow) -> Result<(), DiscoveryError> {
        fixtures::insert_series(&self.pool, row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn insert_collection(&self, row: CollectionRow) -> Result<(), DiscoveryError> {
        fixtures::insert_collection(&self.pool, row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn insert_read_list(&self, row: ReadListRow) -> Result<(), DiscoveryError> {
        fixtures::insert_read_list(&self.pool, row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn insert_book(&self, row: BookRow) -> Result<(), DiscoveryError> {
        fixtures::insert_book(&self.pool, row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn insert_read_progress(&self, row: ReadProgressRow) -> Result<(), DiscoveryError> {
        fixtures::insert_read_progress(&self.pool, row)
            .await
            .map_err(map_sqlx_error)
    }
}

#[async_trait]
impl DiscoveryBrowseService for SqliteDiscoveryAdapter {
    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        request: SeriesBrowseRequest,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        let query = SeriesBrowseQuery::from(request);
        queries::series::list_series_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        request: BooksBrowseRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let query = BooksBrowseQuery::from(request);
        queries::books_media::list_books_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_latest_books(
        &self,
        context: &DiscoveryQueryContext,
        request: LatestBooksRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let feed_query = BooksBrowseQuery {
            filter: BookFilter {
                condition: request.library_ids.map(|ids| {
                    use komga_domain::common_ids::LibraryId;
                    use komga_domain::discovery::{
                        BookCondition, BookValueCondition, CompositeBookCondition, FilterOperator,
                        InclusionCondition,
                    };
                    if ids.len() == 1 {
                        BookCondition::Value(BookValueCondition::LibraryId(
                            InclusionCondition::Include(vec![LibraryId::from(
                                ids.into_iter().next().unwrap(),
                            )]),
                        ))
                    } else {
                        BookCondition::Composite(CompositeBookCondition {
                            operator: FilterOperator::Any,
                            conditions: ids
                                .into_iter()
                                .map(|id| {
                                    BookCondition::Value(BookValueCondition::LibraryId(
                                        InclusionCondition::Include(vec![LibraryId::from(id)]),
                                    ))
                                })
                                .collect(),
                        })
                    }
                }),
                direct_browse_book_id: None,
            },
            sort: vec![],
            search: None,
            page: request.page.page,
            size: request.page.size,
            unpaged: request.page.unpaged,
        };
        queries::books_media::list_books_latest_sqlx(self.pool.clone(), context, &feed_query).await
    }
}

#[async_trait]
impl DiscoveryListService for SqliteDiscoveryAdapter {
    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesBrowseQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        DiscoveryBrowseService::list_series(self, context, query.into()).await
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksBrowseQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        DiscoveryBrowseService::list_books(self, context, query.into()).await
    }

    async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksFeedQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        DiscoveryBrowseService::list_latest_books(self, context, query.into()).await
    }

    async fn list_series_alphabetical_groups(
        &self,
        _context: &DiscoveryQueryContext,
        _filter: SeriesFilter,
        _search: Option<String>,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "alphabetical groups not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_genres(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_tags(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_languages(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_publishers(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_age_ratings(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_sharing_labels(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_series_tags(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_series_release_dates(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<Vec<String>>,
        _: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }

    async fn list_book_tags(
        &self,
        _: &DiscoveryQueryContext,
        _: Option<BookTagScope>,
        _: Option<Vec<String>>,
    ) -> Result<Vec<String>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "facets not supported in runtime adapter".to_string(),
            ),
        ))
    }
}

fn map_sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::Persistence(error.to_string())
}
