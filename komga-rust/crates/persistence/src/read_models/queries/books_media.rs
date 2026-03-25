use komga_application::discovery::{
    BookDetailQuery, BookSiblingQuery, NativeBooksLatestQuery, NativeBooksListQuery,
};
use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, DiscoveryError, DiscoveryQueryContext,
    PageEnvelope,
};
use sqlx::SqlitePool;

use super::{book_detail, books, map_sqlx_error, parse_labels};

#[derive(sqlx::FromRow)]
struct SqlxBookResourceRow {
    id: String,
    library_id: String,
    age_rating: Option<u16>,
    labels: String,
}

impl From<SqlxBookResourceRow> for BookResourceReadModel {
    fn from(value: SqlxBookResourceRow) -> Self {
        Self {
            id: value.id,
            library_id: value.library_id,
            age_rating: value.age_rating,
            labels: parse_labels(&value.labels),
        }
    }
}

pub(in crate::read_models) async fn list_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeBooksListQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    books::list_books_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn list_books_latest_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeBooksLatestQuery,
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
        "SELECT b.id, b.library_id, s.age_rating, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS labels \
         FROM books b \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN series_labels sl ON sl.series_id = s.id \
         WHERE b.id = ? \
         GROUP BY b.id, b.library_id, s.age_rating",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(row.map(BookResourceReadModel::from))
}
