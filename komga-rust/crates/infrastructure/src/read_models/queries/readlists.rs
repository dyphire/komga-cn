#[path = "readlists/books.rs"]
mod books;
#[path = "readlists/browse.rs"]
mod browse;
#[path = "readlists/collections.rs"]
mod collections;
#[path = "readlists/detail.rs"]
mod detail;
#[path = "readlists/sibling.rs"]
mod sibling;

use komga_application::discovery::{
    BookDetailReadModel, BookReadModel, BookReadlistsQuery, CollectionReadModel,
    ReadListDetailQuery, ReadListReadModel, RuntimeReadListBooksQuery, RuntimeReadListsQuery,
    SeriesCollectionsQuery,
};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::map_sqlx_error;
use crate::read_models::filters::{SqlxWhereState, append_clause_sqlx, query_filters_sqlx};

#[derive(sqlx::FromRow)]
struct SqlxReadlistBookRow {
    id: String,
    series_id: String,
    title: String,
}

impl From<SqlxReadlistBookRow> for BookReadModel {
    fn from(value: SqlxReadlistBookRow) -> Self {
        Self {
            id: value.id,
            series_id: value.series_id,
            name: value.title,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxReadListCandidateRow {
    id: String,
    name: String,
}

pub(in crate::read_models) async fn list_readlist_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeReadListBooksQuery,
) -> Result<komga_domain::discovery::PageEnvelope<BookReadModel>, DiscoveryError> {
    books::list_readlist_books_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn list_readlists_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeReadListsQuery,
) -> Result<komga_domain::discovery::PageEnvelope<ReadListReadModel>, DiscoveryError> {
    browse::list_readlists_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn list_book_readlists_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookReadlistsQuery,
) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
    detail::list_book_readlists_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn get_readlist_detail_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &ReadListDetailQuery,
) -> Result<Option<ReadListReadModel>, DiscoveryError> {
    detail::get_readlist_detail_sqlx(pool, context, query).await
}

pub(in crate::read_models) async fn get_readlist_book_sibling_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
    book_id: &str,
    next: bool,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    sibling::get_readlist_book_sibling_sqlx(pool, context, readlist_id, book_id, next).await
}

pub(in crate::read_models) async fn list_series_collections_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &SeriesCollectionsQuery,
) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
    collections::list_series_collections_sqlx(pool, context, query).await
}

async fn visible_readlist_book_ids_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
    allowed_library_ids: Option<&Vec<String>>,
) -> Result<Vec<String>, DiscoveryError> {
    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT rlb.book_id \
         FROM readlist_books rlb \
         JOIN books b ON b.id = rlb.book_id \
         JOIN series s ON s.id = b.series_id",
    );
    let mut state = SqlxWhereState::default();
    query_filters_sqlx(
        &mut builder,
        &mut state,
        "b.library_id",
        allowed_library_ids,
        None,
        None,
        context.restrictions.as_ref(),
        "s",
    );
    append_clause_sqlx("rlb.readlist_id = ", &mut builder, &mut state);
    builder.push_bind(readlist_id);
    builder.push(" ORDER BY rlb.position ASC");

    builder
        .build_query_scalar::<String>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)
}

async fn list_readlist_candidate_rows_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    allowed_library_ids: Option<&Vec<String>>,
    search: Option<&str>,
) -> Result<Vec<SqlxReadListCandidateRow>, DiscoveryError> {
    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT DISTINCT rl.id AS id, rl.name AS name, rl.summary AS summary, \
                rl.ordered AS ordered, rl.created_date AS created_date, \
                rl.last_modified_date AS last_modified_date \
         FROM readlists rl \
         JOIN readlist_books rlb ON rlb.readlist_id = rl.id \
         JOIN books b ON b.id = rlb.book_id \
         JOIN series s ON s.id = b.series_id",
    );
    let mut state = SqlxWhereState::default();
    query_filters_sqlx(
        &mut builder,
        &mut state,
        "b.library_id",
        allowed_library_ids,
        None,
        None,
        context.restrictions.as_ref(),
        "s",
    );

    if let Some(search) = search {
        append_clause_sqlx("(LOWER(rl.name) LIKE ", &mut builder, &mut state);
        builder.push_bind(format!("%{}%", search.to_ascii_lowercase()));
        builder.push(" OR LOWER(rl.summary) LIKE ");
        builder.push_bind(format!("%{}%", search.to_ascii_lowercase()));
        builder.push(")");
    }

    builder.push(" ORDER BY rl.name COLLATE NOCASE ASC");

    builder
        .build_query_as::<SqlxReadListCandidateRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)
}

fn browse_page_size(size: usize) -> usize {
    if size == 0 { 20 } else { size }
}

fn readlist_book_order_sql(ordered: bool) -> &'static str {
    if ordered {
        "MIN(rlb.position) ASC"
    } else {
        "b.metadata_release_date ASC, b.title COLLATE NOCASE ASC"
    }
}
