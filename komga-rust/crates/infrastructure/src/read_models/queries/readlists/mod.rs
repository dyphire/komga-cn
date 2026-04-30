mod books;
mod browse;
mod collections;
mod detail;
mod sibling;

use komga_application::discovery::{
    BookDetailReadModel, BookReadModel, BookReadProgressReadModel, BookReadlistsQuery,
    CollectionReadModel, ReadListBooksQuery, ReadListDetailQuery, ReadListReadModel,
    RuntimeReadListsQuery, SeriesCollectionsQuery,
};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::map_sqlx_error;
use crate::read_models::filters::{SqlxWhereState, append_clause_sqlx, query_filters_sqlx};

#[derive(sqlx::FromRow)]
struct SqlxReadlistBookRow {
    id: String,
    series_id: String,
    series_title: String,
    library_id: String,
    title: String,
    url: String,
    number: i64,
    created: String,
    last_modified: String,
    file_last_modified: String,
    size_bytes: i64,
    media_status: String,
    media_type: String,
    media_pages_count: i64,
    media_comment: String,
    media_epub_divina_compatible: bool,
    media_epub_is_kepub: bool,
    metadata_release_date: Option<String>,
    metadata_title_lock: bool,
    metadata_summary: String,
    metadata_summary_lock: bool,
    metadata_number: String,
    metadata_number_lock: bool,
    metadata_number_sort: f64,
    metadata_number_sort_lock: bool,
    metadata_release_date_lock: bool,
    metadata_authors: String,
    metadata_authors_lock: bool,
    metadata_tags: String,
    metadata_tags_lock: bool,
    metadata_isbn: String,
    metadata_isbn_lock: bool,
    metadata_links: String,
    metadata_links_lock: bool,
    metadata_created: String,
    metadata_last_modified: String,
    read_progress_page: Option<i64>,
    read_progress_completed: Option<bool>,
    read_progress_read_date: Option<String>,
    read_progress_created: Option<String>,
    read_progress_last_modified: Option<String>,
    read_progress_device_id: Option<String>,
    read_progress_device_name: Option<String>,
    deleted: bool,
    file_hash: String,
    oneshot: bool,
}

impl From<SqlxReadlistBookRow> for BookReadModel {
    fn from(value: SqlxReadlistBookRow) -> Self {
        let metadata_title = value.title.clone();

        Self {
            id: value.id,
            series_id: value.series_id,
            series_title: value.series_title,
            library_id: value.library_id,
            name: value.title,
            url: value.url,
            number: value.number as i32,
            created: value.created,
            last_modified: value.last_modified,
            file_last_modified: value.file_last_modified,
            size_bytes: value.size_bytes.max(0) as u64,
            media_status: value.media_status,
            media_type: value.media_type,
            media_pages_count: value.media_pages_count.max(0) as u32,
            media_comment: value.media_comment,
            media_epub_divina_compatible: value.media_epub_divina_compatible,
            media_epub_is_kepub: value.media_epub_is_kepub,
            metadata_title,
            metadata_title_lock: value.metadata_title_lock,
            metadata_summary: value.metadata_summary,
            metadata_summary_lock: value.metadata_summary_lock,
            metadata_number: value.metadata_number,
            metadata_number_lock: value.metadata_number_lock,
            metadata_number_sort: value.metadata_number_sort,
            metadata_number_sort_lock: value.metadata_number_sort_lock,
            metadata_release_date: value.metadata_release_date,
            metadata_release_date_lock: value.metadata_release_date_lock,
            metadata_authors: super::books::parse_metadata_authors(&value.metadata_authors),
            metadata_authors_lock: value.metadata_authors_lock,
            metadata_tags: super::books::parse_csv_values(&value.metadata_tags),
            metadata_tags_lock: value.metadata_tags_lock,
            metadata_isbn: value.metadata_isbn,
            metadata_isbn_lock: value.metadata_isbn_lock,
            metadata_links: super::books::parse_metadata_links(&value.metadata_links),
            metadata_links_lock: value.metadata_links_lock,
            metadata_created: value.metadata_created,
            metadata_last_modified: value.metadata_last_modified,
            read_progress: value
                .read_progress_page
                .map(|page| BookReadProgressReadModel {
                    page: page as i32,
                    completed: value.read_progress_completed.unwrap_or(false),
                    read_date: value.read_progress_read_date,
                    created: value.read_progress_created.unwrap_or_default(),
                    last_modified: value.read_progress_last_modified.unwrap_or_default(),
                    device_id: value.read_progress_device_id.unwrap_or_default(),
                    device_name: value.read_progress_device_name.unwrap_or_default(),
                }),
            deleted: value.deleted,
            file_hash: value.file_hash,
            oneshot: value.oneshot,
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
    query: &ReadListBooksQuery,
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
        r#"
        SELECT rlb.book_id
        FROM readlist_books rlb
        JOIN books b ON b.id = rlb.book_id
        JOIN series s ON s.id = b.series_id
        "#,
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
        r#"
        SELECT DISTINCT rl.id AS id, rl.name AS name, rl.summary AS summary,
               rl.ordered AS ordered, rl.created_date AS created_date,
               rl.last_modified_date AS last_modified_date
        FROM readlists rl
        JOIN readlist_books rlb ON rlb.readlist_id = rl.id
        JOIN books b ON b.id = rlb.book_id
        JOIN series s ON s.id = b.series_id
        "#,
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
