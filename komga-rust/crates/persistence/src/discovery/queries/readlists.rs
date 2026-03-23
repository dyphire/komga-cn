use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, NativeReadListBooksQuery, NativeReadListsQuery,
    ReadListDetailQuery,
};
use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, DiscoveryError, DiscoveryQueryContext, PageEnvelope,
    ReadListReadModel,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::book_detail::get_book_detail_sqlx;
use super::books::apply_books_filters_sqlx;
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
    let allowed = effective_library_ids(context, query.library_ids.as_deref());
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(
            vec![],
            query.page,
            query.size.max(1),
            0,
        ));
    }

    let ordered = sqlx::query_scalar::<_, bool>("SELECT ordered FROM readlists WHERE id = ?")
        .bind(&query.readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;
    let Some(ordered) = ordered else {
        return Ok(PageEnvelope::from_slice(
            vec![],
            query.page,
            query.size.max(1),
            0,
        ));
    };

    let mut count_builder = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(DISTINCT b.id) FROM readlist_books rlb JOIN books b ON b.id = rlb.book_id JOIN series s ON s.id = b.series_id",
    );
    let mut count_state = SqlxWhereState::default();
    apply_books_filters_sqlx(
        &mut count_builder,
        &mut count_state,
        context,
        allowed.as_ref(),
        None,
        query.deleted,
        None,
        true,
        query.tags.as_deref(),
        query.read_statuses.as_deref(),
        None,
        query.media_statuses.as_deref(),
        query.authors.as_deref(),
        None,
        None,
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
    apply_books_filters_sqlx(
        &mut select_builder,
        &mut select_state,
        context,
        allowed.as_ref(),
        None,
        query.deleted,
        None,
        true,
        query.tags.as_deref(),
        query.read_statuses.as_deref(),
        None,
        query.media_statuses.as_deref(),
        query.authors.as_deref(),
        None,
        None,
    );
    append_clause_sqlx("rlb.readlist_id = ", &mut select_builder, &mut select_state);
    select_builder.push_bind(query.readlist_id.clone());
    select_builder.push(
        " GROUP BY b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, \
            b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title \
          ORDER BY ",
    );
    select_builder.push(readlist_book_order_sql(ordered));

    let (envelope_page, envelope_size) = if query.unpaged {
        (0, total_elements.max(1))
    } else {
        let safe_size = query.size.max(1);
        let offset = query.page.saturating_mul(safe_size);
        select_builder.push(" LIMIT ");
        select_builder.push_bind(safe_size as i64);
        select_builder.push(" OFFSET ");
        select_builder.push_bind(offset as i64);
        (query.page, safe_size)
    };

    let rows = select_builder
        .build_query_as::<SqlxReadlistBookRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;

    Ok(PageEnvelope::from_slice(
        rows.into_iter().map(BookReadModel::from).collect(),
        envelope_page,
        envelope_size,
        total_elements,
    ))
}

pub(in crate::discovery) async fn list_readlists_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeReadListsQuery,
) -> Result<PageEnvelope<ReadListReadModel>, DiscoveryError> {
    let candidate_library_ids = effective_library_ids(context, query.library_ids.as_deref());
    if candidate_library_ids.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(vec![], query.page, browse_page_size(query.size), 0));
    }

    let candidate_rows = list_readlist_candidate_rows_sqlx(
        pool.clone(),
        context,
        candidate_library_ids.as_ref(),
    )
    .await?;
    let total_elements = candidate_rows.len();

    let page_size = browse_page_size(query.size);
    let offset = query.page.saturating_mul(page_size);
    let page_rows = if offset >= total_elements {
        vec![]
    } else {
        candidate_rows
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect::<Vec<_>>()
    };

    let visible_library_ids = effective_library_ids(context, None);
    let mut readlists = vec![];
    for candidate in page_rows {
        let visible_book_ids = visible_readlist_book_ids_sqlx(
            pool.clone(),
            context,
            &candidate.id,
            visible_library_ids.as_ref(),
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

    Ok(PageEnvelope::from_slice(
        readlists,
        query.page,
        page_size,
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

pub(in crate::discovery) async fn get_readlist_detail_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &ReadListDetailQuery,
) -> Result<Option<ReadListReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let candidate = sqlx::query_as::<_, SqlxReadListCandidateRow>(
        "SELECT id, name, summary, ordered, created_date, last_modified_date FROM readlists WHERE id = ?",
    )
    .bind(&query.readlist_id)
    .fetch_optional(&pool)
    .await
    .map_err(map_sqlx_error)?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };

    let visible_book_ids = visible_readlist_book_ids_sqlx(
        pool.clone(),
        context,
        &candidate.id,
        allowed.as_ref(),
    )
    .await?;

    let total_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM readlist_books WHERE readlist_id = ?")
            .bind(candidate.id.clone())
            .fetch_one(&pool)
            .await
            .map_err(map_sqlx_error)?;

    if visible_book_ids.is_empty() && total_count > 0 {
        return Ok(None);
    }

    Ok(Some(ReadListReadModel {
        id: candidate.id,
        name: candidate.name,
        summary: candidate.summary,
        ordered: candidate.ordered,
        created_date: candidate.created_date,
        last_modified_date: candidate.last_modified_date,
        filtered: (visible_book_ids.len() as i64) < total_count,
        book_ids: visible_book_ids,
    }))
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
            page: 0,
            size: 20,
            unpaged: true,
            library_ids: None,
            deleted: None,
            tags: None,
            read_statuses: None,
            media_statuses: None,
            authors: None,
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

async fn list_readlist_candidate_rows_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    allowed_library_ids: Option<&Vec<String>>,
) -> Result<Vec<SqlxReadListCandidateRow>, DiscoveryError> {
    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT DISTINCT rl.id AS id, rl.name AS name, rl.summary AS summary, rl.ordered AS ordered, rl.created_date AS created_date, rl.last_modified_date AS last_modified_date \
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
