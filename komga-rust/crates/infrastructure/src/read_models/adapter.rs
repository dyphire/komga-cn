use async_trait::async_trait;
use komga_application::discovery::{
    BookReadModel, BookTagScope, BooksBrowseRequest, DiscoveryBrowseService, DiscoveryFacetService,
    LatestBooksRequest, SeriesAlphabeticalGroupsRequest, SeriesBrowseRequest, SeriesReadModel,
};
use komga_domain::discovery::{
    BookFilter, DiscoveryError, DiscoveryQueryContext, PageEnvelope, UnsupportedDiscoverySemantics,
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
        queries::series::list_series_sqlx(self.pool.clone(), context, &request).await
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        request: BooksBrowseRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        queries::books_media::list_books_sqlx(self.pool.clone(), context, &request).await
    }

    async fn list_latest_books(
        &self,
        context: &DiscoveryQueryContext,
        request: LatestBooksRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let feed_query = BooksBrowseRequest {
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
            page: request.page,
        };
        queries::books_media::list_books_latest_sqlx(self.pool.clone(), context, &feed_query).await
    }

    async fn list_series_alphabetical_groups(
        &self,
        _context: &DiscoveryQueryContext,
        _request: SeriesAlphabeticalGroupsRequest,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError> {
        Err(DiscoveryError::UnsupportedSemantics(
            UnsupportedDiscoverySemantics::UnsupportedSeriesSort(
                "alphabetical groups not supported in runtime adapter".to_string(),
            ),
        ))
    }
}

#[async_trait]
impl DiscoveryFacetService for SqliteDiscoveryAdapter {
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

#[cfg(test)]
mod tests {
    use komga_application::discovery::{DiscoveryBrowseService, SeriesBrowseRequest};
    use komga_domain::common_ids::{ReadListId, SeriesId};
    use komga_domain::discovery::{
        BookCondition, BookFilter, BookValueCondition, CompositeBookCondition,
        CompositeSeriesCondition, DiscoveryQueryContext, FilterOperator, InclusionCondition,
        SeriesCondition, SeriesFilter, SeriesValueCondition, StringCondition,
    };

    use super::*;

    fn query_context() -> DiscoveryQueryContext {
        DiscoveryQueryContext {
            user_id: None,
            is_admin: true,
            authorized_library_ids: None,
            restrictions: None,
        }
    }

    fn query_with(condition: BookCondition) -> BooksBrowseRequest {
        BooksBrowseRequest {
            filter: BookFilter {
                condition: Some(condition),
                direct_browse_book_id: None,
            },
            ..BooksBrowseRequest::default()
        }
    }

    fn series_query_with(condition: SeriesCondition) -> SeriesBrowseRequest {
        SeriesBrowseRequest {
            filter: SeriesFilter {
                condition: Some(condition),
            },
            ..SeriesBrowseRequest::default()
        }
    }

    async fn insert_library_series_and_books(
        adapter: &SqliteDiscoveryAdapter,
        mut series: SeriesRow,
        books: Vec<BookRow>,
    ) {
        adapter
            .insert_library(LibraryRow::new("library-1", "Library 1"))
            .await
            .unwrap();
        series.library_id = "library-1".to_string();
        adapter.insert_series(series).await.unwrap();
        for book in books {
            adapter.insert_book(book).await.unwrap();
        }
    }

    #[tokio::test]
    async fn list_books_scoped_to_series_includes_oneshot_series_books() {
        let adapter = SqliteDiscoveryAdapter::new().await.unwrap();
        let mut series = SeriesRow::new("series-1", "library-1", "One Shot");
        series.oneshot = true;
        insert_library_series_and_books(
            &adapter,
            series,
            vec![BookRow::new("book-1", "series-1", "library-1", "Book 1")],
        )
        .await;

        let page = DiscoveryBrowseService::list_books(
            &adapter,
            &query_context(),
            query_with(BookCondition::Value(BookValueCondition::SeriesId(
                InclusionCondition::Include(vec![SeriesId::from("series-1")]),
            ))),
        )
        .await
        .unwrap();

        assert_eq!(
            page.content
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["book-1"]
        );
    }

    #[tokio::test]
    async fn list_books_tag_filter_matches_book_tags() {
        let adapter = SqliteDiscoveryAdapter::new().await.unwrap();
        insert_library_series_and_books(
            &adapter,
            SeriesRow::new("series-1", "library-1", "Series 1"),
            vec![
                BookRow {
                    tags: vec!["favorite".to_string()],
                    ..BookRow::new("book-1", "series-1", "library-1", "Book 1")
                },
                BookRow {
                    tags: vec!["other".to_string()],
                    ..BookRow::new("book-2", "series-1", "library-1", "Book 2")
                },
            ],
        )
        .await;

        let page = DiscoveryBrowseService::list_books(
            &adapter,
            &query_context(),
            query_with(BookCondition::Value(BookValueCondition::Tag(
                StringCondition::Exact(InclusionCondition::Include(vec!["favorite".to_string()])),
            ))),
        )
        .await
        .unwrap();

        assert_eq!(
            page.content
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["book-1"]
        );
    }

    #[tokio::test]
    async fn list_books_readlist_filter_matches_readlist_books() {
        let adapter = SqliteDiscoveryAdapter::new().await.unwrap();
        insert_library_series_and_books(
            &adapter,
            SeriesRow::new("series-1", "library-1", "Series 1"),
            vec![
                BookRow::new("book-1", "series-1", "library-1", "Book 1"),
                BookRow::new("book-2", "series-1", "library-1", "Book 2"),
            ],
        )
        .await;
        adapter
            .insert_read_list(ReadListRow {
                book_ids: vec!["book-2".to_string()],
                ..ReadListRow::new("readlist-1", "Read List 1")
            })
            .await
            .unwrap();

        let page = DiscoveryBrowseService::list_books(
            &adapter,
            &query_context(),
            query_with(BookCondition::Value(BookValueCondition::ReadListId(
                InclusionCondition::Include(vec![ReadListId::from("readlist-1")]),
            ))),
        )
        .await
        .unwrap();

        assert_eq!(
            page.content
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["book-2"]
        );
    }

    #[tokio::test]
    async fn list_books_any_condition_combines_children_with_or() {
        let adapter = SqliteDiscoveryAdapter::new().await.unwrap();
        insert_library_series_and_books(
            &adapter,
            SeriesRow::new("series-1", "library-1", "Series 1"),
            vec![
                BookRow {
                    tags: vec!["favorite".to_string()],
                    ..BookRow::new("book-1", "series-1", "library-1", "Book 1")
                },
                BookRow {
                    tags: vec!["queued".to_string()],
                    ..BookRow::new("book-2", "series-1", "library-1", "Book 2")
                },
                BookRow {
                    tags: vec!["other".to_string()],
                    ..BookRow::new("book-3", "series-1", "library-1", "Book 3")
                },
            ],
        )
        .await;

        let tag_condition = |tag: &str| {
            BookCondition::Value(BookValueCondition::Tag(StringCondition::Exact(
                InclusionCondition::Include(vec![tag.to_string()]),
            )))
        };
        let page = DiscoveryBrowseService::list_books(
            &adapter,
            &query_context(),
            query_with(BookCondition::Composite(CompositeBookCondition {
                operator: FilterOperator::Any,
                conditions: vec![tag_condition("favorite"), tag_condition("queued")],
            })),
        )
        .await
        .unwrap();

        assert_eq!(
            page.content
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["book-1", "book-2"]
        );
    }

    #[tokio::test]
    async fn list_series_any_condition_combines_children_with_or() {
        let adapter = SqliteDiscoveryAdapter::new().await.unwrap();
        adapter
            .insert_library(LibraryRow::new("library-1", "Library 1"))
            .await
            .unwrap();
        adapter
            .insert_series(SeriesRow {
                tags: vec!["favorite".to_string()],
                ..SeriesRow::new("series-1", "library-1", "Series 1")
            })
            .await
            .unwrap();
        adapter
            .insert_series(SeriesRow {
                tags: vec!["queued".to_string()],
                ..SeriesRow::new("series-2", "library-1", "Series 2")
            })
            .await
            .unwrap();
        adapter
            .insert_series(SeriesRow {
                tags: vec!["other".to_string()],
                ..SeriesRow::new("series-3", "library-1", "Series 3")
            })
            .await
            .unwrap();

        let tag_condition = |tag: &str| {
            SeriesCondition::Value(SeriesValueCondition::Tag(StringCondition::Exact(
                InclusionCondition::Include(vec![tag.to_string()]),
            )))
        };
        let page = DiscoveryBrowseService::list_series(
            &adapter,
            &query_context(),
            series_query_with(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::Any,
                conditions: vec![tag_condition("favorite"), tag_condition("queued")],
            })),
        )
        .await
        .unwrap();

        assert_eq!(
            page.content
                .iter()
                .map(|series| series.id.as_str())
                .collect::<Vec<_>>(),
            vec!["series-1", "series-2"]
        );
    }
}
