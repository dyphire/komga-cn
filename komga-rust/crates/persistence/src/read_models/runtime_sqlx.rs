use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, DiscoveryQueryRepository,
    NativeBooksLatestQuery, NativeBooksListQuery, NativeReadListBooksQuery, NativeReadListsQuery,
    NativeSeriesListQuery, SeriesCollectionsQuery, SeriesDetailQuery,
};
use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel, DiscoveryError,
    DiscoveryQueryContext, LibraryReadModel, PageEnvelope, ReadListReadModel,
    SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};
use sqlx::SqlitePool;

#[path = "runtime_sqlx/store_rows.rs"]
mod store_rows;

use super::{BookRow, LibraryRow, SeriesRow};
use crate::read_models::queries;
use crate::read_models::queries::{book_detail, readlists};
use crate::sqlite::{SqliteTempPool, setup};

#[derive(Clone)]
pub struct SqlxRuntimeDiscoveryAdapter {
    pool: SqlitePool,
}

impl SqlxRuntimeDiscoveryAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

pub struct SqlxRuntimeDiscoveryStore {
    temp_pool: SqliteTempPool,
}

impl SqlxRuntimeDiscoveryStore {
    pub async fn new(case_id: &str) -> Result<Self, DiscoveryError> {
        let temp_pool = SqliteTempPool::new(case_id).await.map_err(map_sqlx_error)?;
        setup::bootstrap_read_model_pool(temp_pool.pool())
            .await
            .map_err(map_sqlx_error)?;
        Ok(Self { temp_pool })
    }

    pub fn adapter(&self) -> SqlxRuntimeDiscoveryAdapter {
        SqlxRuntimeDiscoveryAdapter::new(self.temp_pool.pool().clone())
    }

    pub async fn insert_library(&self, row: LibraryRow) -> Result<(), DiscoveryError> {
        store_rows::insert_library_row(self.temp_pool.pool(), row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn insert_series(&self, row: SeriesRow) -> Result<(), DiscoveryError> {
        store_rows::insert_series_row(self.temp_pool.pool(), row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn insert_book(&self, row: BookRow) -> Result<(), DiscoveryError> {
        store_rows::insert_book_row(self.temp_pool.pool(), row)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn cleanup(self) {
        self.temp_pool.cleanup().await;
    }
}

impl DiscoveryQueryRepository for SqlxRuntimeDiscoveryAdapter {
    async fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
        queries::list_libraries_sqlx(self.pool.clone(), context).await
    }

    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeSeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        queries::list_series_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        queries::books::list_books_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        queries::books::list_books_latest_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListBooksQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        readlists::list_readlist_books_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListsQuery,
    ) -> Result<PageEnvelope<ReadListReadModel>, DiscoveryError> {
        readlists::list_readlists_sqlx(self.pool.clone(), context, &query).await
    }

    async fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
        queries::resolve_series_resource_sqlx(self.pool.clone(), series_id).await
    }

    async fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
        queries::get_series_detail_sqlx(self.pool.clone(), context, &query).await
    }

    async fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
        queries::resolve_book_resource_sqlx(self.pool.clone(), book_id).await
    }

    async fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        book_detail::get_book_detail_sqlx(self.pool.clone(), context, &query).await
    }

    async fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        book_detail::get_book_sibling_previous_sqlx(self.pool.clone(), context, &query).await
    }

    async fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        book_detail::get_book_sibling_next_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
        readlists::list_book_readlists_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
        queries::list_series_collections_sqlx(self.pool.clone(), context, &query).await
    }
}

fn map_sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::Persistence(error.to_string())
}
