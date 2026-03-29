use komga_application::discovery::{
    BookDetailQuery, BookDetailReadModel, BookReadModel, BookResourceReadModel, BookSiblingQuery,
    RuntimeBooksLatestQuery, RuntimeBooksListQuery,
};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, PageEnvelope};
use sqlx::SqlitePool;

use super::{book_detail, books, map_sqlx_error};

#[derive(sqlx::FromRow)]
struct SqlxBookResourceRow {
    id: String,
    url: String,
}

impl From<SqlxBookResourceRow> for BookResourceReadModel {
    fn from(value: SqlxBookResourceRow) -> Self {
        Self {
            id: value.id,
            url: value.url,
        }
    }
}

pub(in crate::read_models) async fn list_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeBooksListQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    books::list_books_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn list_books_latest_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeBooksLatestQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    books::list_books_latest_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn get_book_detail_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookDetailQuery,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    book_detail::get_book_detail_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn get_book_sibling_previous_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookSiblingQuery,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    book_detail::get_book_sibling_previous_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn get_book_sibling_next_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookSiblingQuery,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    book_detail::get_book_sibling_next_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn resolve_book_resource_sqlx(
    pool: SqlitePool,
    book_id: &str,
) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
    let row = sqlx::query_as::<_, SqlxBookResourceRow>(
        "SELECT b.id, b.url \
         FROM books b \
         WHERE b.id = ?",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(row.map(BookResourceReadModel::from))
}
