use komga_application::discovery::{BookDetailQuery, BookSiblingQuery};
use komga_domain::discovery::{
    BookDetailReadModel, DiscoveryError, DiscoveryQueryContext, ReadProgressReadModel,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::{map_sqlx_error, parse_labels};
use crate::discovery::filters::{
    SqlxWhereState, append_clause_sqlx, append_in_clause_sqlx, apply_restrictions_sqlx,
    effective_library_ids,
};

#[derive(sqlx::FromRow)]
struct SqlxBookDetailRow {
    id: String,
    series_id: String,
    library_id: String,
    title: String,
    url: String,
    number_sort: i64,
    created: String,
    last_modified: String,
    file_last_modified: String,
    size_bytes: i64,
    media_status: String,
    media_type: String,
    media_pages_count: i64,
    metadata_release_date: Option<String>,
    deleted: bool,
    oneshot: bool,
    series_title: String,
    metadata_authors: String,
    metadata_tags: String,
    rp_page: Option<i64>,
    rp_completed: Option<bool>,
    rp_read_date: Option<String>,
    rp_created: Option<String>,
    rp_last_modified: Option<String>,
    rp_device_id: Option<String>,
    rp_device_name: Option<String>,
}

impl From<SqlxBookDetailRow> for BookDetailReadModel {
    fn from(value: SqlxBookDetailRow) -> Self {
        let read_progress = match (
            value.rp_page,
            value.rp_completed,
            value.rp_read_date,
            value.rp_created,
            value.rp_last_modified,
            value.rp_device_id,
            value.rp_device_name,
        ) {
            (
                Some(page),
                Some(completed),
                Some(read_date),
                Some(created),
                Some(last_modified),
                Some(device_id),
                Some(device_name),
            ) => Some(ReadProgressReadModel {
                page: page as u32,
                completed,
                read_date,
                created,
                last_modified,
                device_id,
                device_name,
            }),
            _ => None,
        };

        Self {
            id: value.id,
            series_id: value.series_id,
            library_id: value.library_id,
            name: value.title.clone(),
            url: value.url,
            number: value.number_sort as i32,
            created: value.created.clone(),
            last_modified: value.last_modified.clone(),
            file_last_modified: value.file_last_modified,
            size_bytes: value.size_bytes as u64,
            media_status: value.media_status,
            media_type: value.media_type,
            media_pages_count: value.media_pages_count as u32,
            metadata_release_date: value.metadata_release_date,
            deleted: value.deleted,
            oneshot: value.oneshot,
            series_title: value.series_title,
            metadata_authors: parse_labels(&value.metadata_authors),
            metadata_tags: parse_labels(&value.metadata_tags),
            read_progress,
            media_comment: String::new(),
            metadata_title: value.title,
            metadata_summary: String::new(),
            metadata_number: (value.number_sort as i32).to_string(),
            metadata_number_sort: value.number_sort as f64,
            metadata_isbn: String::new(),
            metadata_created: value.created,
            metadata_last_modified: value.last_modified,
            file_hash: String::new(),
        }
    }
}

pub(in crate::discovery) async fn get_book_detail_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookDetailQuery,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    fetch_book_detail_sqlx(pool, context, &query.book_id).await
}

pub(in crate::discovery) async fn get_book_sibling_previous_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookSiblingQuery,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    get_book_sibling_sqlx(pool, context, &query.book_id, false).await
}

pub(in crate::discovery) async fn get_book_sibling_next_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookSiblingQuery,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    get_book_sibling_sqlx(pool, context, &query.book_id, true).await
}

async fn get_book_sibling_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    book_id: &str,
    next: bool,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    let anchor = sqlx::query_as::<_, (String, i64)>("SELECT series_id, number_sort FROM books WHERE id = ?")
        .bind(book_id)
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;

    let Some((series_id, number_sort)) = anchor else {
        return Ok(None);
    };

    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let mut builder = QueryBuilder::<Sqlite>::new("SELECT b.id FROM books b JOIN series s ON s.id = b.series_id");
    let mut state = SqlxWhereState::default();
    append_clause_sqlx("b.series_id = ", &mut builder, &mut state);
    builder.push_bind(series_id);
    if let Some(allowed_ids) = allowed.as_ref() {
        append_in_clause_sqlx("b.library_id", allowed_ids, &mut builder, &mut state);
    }
    if let Some(restrictions) = context.restrictions.as_ref() {
        apply_restrictions_sqlx("s", restrictions, &mut builder, &mut state);
    }
    if next {
        append_clause_sqlx("b.number_sort > ", &mut builder, &mut state);
        builder.push_bind(number_sort);
    } else {
        append_clause_sqlx("b.number_sort < ", &mut builder, &mut state);
        builder.push_bind(number_sort);
    }
    builder.push(" ORDER BY b.number_sort ");
    builder.push(if next { "ASC" } else { "DESC" });
    builder.push(", b.title COLLATE NOCASE ");
    builder.push(if next { "ASC" } else { "DESC" });
    builder.push(" LIMIT 1");

    let sibling_id = builder
        .build_query_scalar::<String>()
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;

    let Some(sibling_id) = sibling_id else {
        return Ok(None);
    };

    fetch_book_detail_sqlx(pool, context, &sibling_id).await
}

pub(in crate::discovery) async fn fetch_book_detail_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    book_id: &str,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT \
            b.id AS id, b.series_id AS series_id, b.library_id AS library_id, b.title AS title, b.url AS url, b.number_sort AS number_sort, \
            b.created AS created, b.last_modified AS last_modified, b.file_last_modified AS file_last_modified, b.size_bytes AS size_bytes, \
            b.media_status AS media_status, b.media_type AS media_type, b.media_pages_count AS media_pages_count, b.metadata_release_date AS metadata_release_date, \
            b.deleted AS deleted, b.oneshot AS oneshot, s.title AS series_title, \
            COALESCE(GROUP_CONCAT(DISTINCT ba.author), '') AS metadata_authors, COALESCE(GROUP_CONCAT(DISTINCT bt.tag), '') AS metadata_tags, \
            rp.page AS rp_page, rp.completed AS rp_completed, rp.read_date AS rp_read_date, rp.created AS rp_created, rp.last_modified AS rp_last_modified, rp.device_id AS rp_device_id, rp.device_name AS rp_device_name \
         FROM books b \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN book_authors ba ON ba.book_id = b.id \
         LEFT JOIN book_tags bt ON bt.book_id = b.id \
         LEFT JOIN read_progress rp ON rp.book_id = b.id AND rp.user_id = ",
    );
    let user_id = context.user_id.clone().unwrap_or_default();
    builder.push_bind(user_id);

    let mut state = SqlxWhereState::default();
    append_clause_sqlx("b.id = ", &mut builder, &mut state);
    builder.push_bind(book_id);
    if let Some(allowed_ids) = allowed.as_ref() {
        append_in_clause_sqlx("b.library_id", allowed_ids, &mut builder, &mut state);
    }
    if let Some(restrictions) = context.restrictions.as_ref() {
        apply_restrictions_sqlx("s", restrictions, &mut builder, &mut state);
    }
    builder.push(
        " GROUP BY \
            b.id, b.series_id, b.library_id, b.title, b.url, b.number_sort, b.created, b.last_modified, b.file_last_modified, \
            b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title, \
            rp.page, rp.completed, rp.read_date, rp.created, rp.last_modified, rp.device_id, rp.device_name",
    );

    let row = builder
        .build_query_as::<SqlxBookDetailRow>()
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;

    Ok(row.map(BookDetailReadModel::from))
}
