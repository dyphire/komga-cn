use komga_application::discovery::{BookReadModel, BooksBrowseQuery};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, PageEnvelope};
use sqlx::SqlitePool;

use super::books;

pub(in crate::read_models) async fn list_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    books::list_books_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn list_books_latest_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    books::list_books_latest_sqlx(pool, context, query).await
}
