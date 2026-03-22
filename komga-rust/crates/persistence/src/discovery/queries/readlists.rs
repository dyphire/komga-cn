use komga_application::discovery::{BookDetailQuery, BookReadlistsQuery, NativeReadListBooksQuery};
use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, DiscoveryError, DiscoveryQueryContext, PageEnvelope,
    ReadListReadModel,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::book_detail::get_book_detail_sqlx;
use super::{map_sqlx_error, parse_labels};
use crate::discovery::filters::{
    SqlxWhereState, append_clause_sqlx, effective_library_ids, query_filters_sqlx,
};

#[derive(sqlx::FromRow)]
struct SqlxReadlistBookRow {
    id: String,
    series_id: String,
    library_id: String,
    title: String,
    url: String,
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
    labels: String,
}

impl From<SqlxReadlistBookRow> for BookReadModel {
    fn from(value: SqlxReadlistBookRow) -> Self {
        Self {
            id: value.id,
            series_id: value.series_id,
            library_id: value.library_id,
            title: value.title,
            url: value.url,
            created: value.created,
            last_modified: value.last_modified,
            file_last_modified: value.file_last_modified,
            size_bytes: value.size_bytes as u64,
            media_status: value.media_status,
            media_type: value.media_type,
            media_pages_count: value.media_pages_count as u32,
            metadata_release_date: value.metadata_release_date,
            deleted: value.deleted,
            oneshot: value.oneshot,
            series_title: value.series_title,
            labels: parse_labels(&value.labels),
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxReadListCandidateRow {
    id: String,
    name: String,
    summary: String,
    ordered: bool,
    created_date: String,
    last_modified_date: String,
}

pub(in crate::discovery) async fn list_readlist_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeReadListBooksQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(vec![], 0, 1, 0));
    }

    let ordered = sqlx::query_scalar::<_, bool>("SELECT ordered FROM readlists WHERE id = ?")
        .bind(&query.readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;
    let Some(ordered) = ordered else {
        return Ok(PageEnvelope::from_slice(vec![], 0, 1, 0));
    };

    let mut count_builder = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(DISTINCT b.id) FROM readlist_books rlb JOIN books b ON b.id = rlb.book_id JOIN series s ON s.id = b.series_id",
    );
    let mut count_state = SqlxWhereState::default();
    query_filters_sqlx(
        &mut count_builder,
        &mut count_state,
        "b.library_id",
        allowed.as_ref(),
        None,
        None,
        context.restrictions.as_ref(),
        "s",
    );
    append_clause_sqlx("rlb.readlist_id = ", &mut count_builder, &mut count_state);
    count_builder.push_bind(query.readlist_id.clone());
    let total_elements = count_builder
        .build_query_scalar::<i64>()
        .fetch_one(&pool)
        .await
        .map_err(map_sqlx_error)? as usize;

    let mut select_builder = QueryBuilder::<Sqlite>::new(
        "SELECT \
            b.id AS id, b.series_id AS series_id, b.library_id AS library_id, b.title AS title, b.url AS url, \
            b.created AS created, b.last_modified AS last_modified, b.file_last_modified AS file_last_modified, \
            b.size_bytes AS size_bytes, b.media_status AS media_status, b.media_type AS media_type, b.media_pages_count AS media_pages_count, \
            b.metadata_release_date AS metadata_release_date, b.deleted AS deleted, b.oneshot AS oneshot, \
            s.title AS series_title, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS labels \
         FROM readlist_books rlb \
         JOIN books b ON b.id = rlb.book_id \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN series_labels sl ON sl.series_id = s.id",
    );
    let mut select_state = SqlxWhereState::default();
    query_filters_sqlx(
        &mut select_builder,
        &mut select_state,
        "b.library_id",
        allowed.as_ref(),
        None,
        None,
        context.restrictions.as_ref(),
        "s",
    );
    append_clause_sqlx("rlb.readlist_id = ", &mut select_builder, &mut select_state);
    select_builder.push_bind(query.readlist_id.clone());
    select_builder.push(
        " GROUP BY b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, \
            b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title \
          ORDER BY ",
    );
    select_builder.push(readlist_book_order_sql(ordered));

    let rows = select_builder
        .build_query_as::<SqlxReadlistBookRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;

    Ok(PageEnvelope::from_slice(
        rows.into_iter().map(BookReadModel::from).collect(),
        0,
        total_elements.max(1),
        total_elements,
    ))
}

pub(in crate::discovery) async fn list_book_readlists_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BookReadlistsQuery,
) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    let candidates = sqlx::query_as::<_, SqlxReadListCandidateRow>(
        "SELECT DISTINCT rl.id AS id, rl.name AS name, rl.summary AS summary, rl.ordered AS ordered, rl.created_date AS created_date, rl.last_modified_date AS last_modified_date \
         FROM readlists rl \
         JOIN readlist_books rlb ON rlb.readlist_id = rl.id \
         WHERE rlb.book_id = ? \
         ORDER BY rl.name COLLATE NOCASE ASC",
    )
    .bind(&query.book_id)
    .fetch_all(&pool)
    .await
    .map_err(map_sqlx_error)?;

    let mut readlists = vec![];
    for candidate in candidates {
        let visible_book_ids = visible_readlist_book_ids_sqlx(
            pool.clone(),
            context,
            &candidate.id,
            allowed.as_ref(),
        )
        .await?;
        if visible_book_ids.is_empty() {
            continue;
        }

        let total_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM readlist_books WHERE readlist_id = ?")
                .bind(candidate.id.clone())
                .fetch_one(&pool)
                .await
                .map_err(map_sqlx_error)?;

        readlists.push(ReadListReadModel {
            id: candidate.id,
            name: candidate.name,
            summary: candidate.summary,
            ordered: candidate.ordered,
            book_ids: visible_book_ids.clone(),
            created_date: candidate.created_date,
            last_modified_date: candidate.last_modified_date,
            filtered: (visible_book_ids.len() as i64) < total_count,
        });
    }

    Ok(readlists)
}

pub(in crate::discovery) async fn get_readlist_book_sibling_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
    book_id: &str,
    next: bool,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    let page = list_readlist_books_sqlx(
        pool.clone(),
        context,
        &NativeReadListBooksQuery {
            readlist_id: readlist_id.to_string(),
        },
    )
    .await?;
    let visible_book_ids = page
        .content
        .iter()
        .map(|it| it.id.as_str())
        .collect::<Vec<_>>();

    let Some(current_index) = visible_book_ids.iter().position(|id| *id == book_id) else {
        return Ok(None);
    };

    let sibling_id = if next {
        visible_book_ids.get(current_index + 1)
    } else if current_index == 0 {
        None
    } else {
        visible_book_ids.get(current_index - 1)
    };

    let Some(sibling_id) = sibling_id else {
        return Ok(None);
    };

    get_book_detail_sqlx(
        pool,
        context,
        &BookDetailQuery {
            book_id: (*sibling_id).to_string(),
        },
    )
    .await
}

async fn visible_readlist_book_ids_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
    allowed_library_ids: Option<&Vec<String>>,
) -> Result<Vec<String>, DiscoveryError> {
    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT rlb.book_id FROM readlist_books rlb JOIN books b ON b.id = rlb.book_id JOIN series s ON s.id = b.series_id",
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

fn readlist_book_order_sql(ordered: bool) -> &'static str {
    if ordered {
        "MIN(rlb.position) ASC"
    } else {
        "b.metadata_release_date ASC, b.title COLLATE NOCASE ASC"
    }
}
