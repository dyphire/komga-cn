use komga_application::discovery::{NativeBooksLatestQuery, NativeBooksListQuery};
use komga_domain::discovery::{BookReadModel, DiscoveryError, DiscoveryQueryContext, PageEnvelope};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::{map_sqlx_error, parse_labels};
use crate::discovery::filters::{
    SqlxWhereState, append_clause_sqlx, append_string_set_filter_sqlx, effective_library_ids,
    query_filters_sqlx,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BookOrdering {
    TitleAsc,
    CreatedDateDesc,
    MetadataReleaseDateDesc,
    NumberSortAsc,
    LastModifiedDesc,
}

#[derive(sqlx::FromRow)]
struct SqlxBookListRow {
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

impl From<SqlxBookListRow> for BookReadModel {
    fn from(value: SqlxBookListRow) -> Self {
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

pub(in crate::discovery) async fn list_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeBooksListQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    list_books_sqlx_common(
        pool,
        context,
        query.page,
        query.size,
        query.library_ids.as_deref(),
        query.series_ids.as_deref(),
        query.deleted,
        query.oneshot,
        query.tags.as_deref(),
        query.read_statuses.as_deref(),
        query.media_profiles.as_deref(),
        query.media_statuses.as_deref(),
        query.authors.as_deref(),
        query.release_dates.as_deref(),
        query.search.as_deref(),
        query.unpaged,
        book_ordering_from_sorts(&query.sort),
    )
    .await
}

pub(in crate::discovery) async fn list_books_latest_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeBooksLatestQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    list_books_sqlx_common(
        pool,
        context,
        query.page,
        query.size,
        query.library_ids.as_deref(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        query.unpaged,
        BookOrdering::LastModifiedDesc,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn list_books_sqlx_common(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    page: usize,
    size: usize,
    requested_library_ids: Option<&[String]>,
    requested_series_ids: Option<&[String]>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    tags: Option<&[String]>,
    read_statuses: Option<&[String]>,
    media_profiles: Option<&[String]>,
    media_statuses: Option<&[String]>,
    authors: Option<&[String]>,
    release_dates: Option<&[String]>,
    search: Option<&str>,
    unpaged: bool,
    ordering: BookOrdering,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, requested_library_ids);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(vec![], page, size.max(1), 0));
    }

    let scoped_to_series = requested_series_ids.is_some_and(|series_ids| !series_ids.is_empty());

    let mut count_builder = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(DISTINCT b.id) FROM books b JOIN series s ON s.id = b.series_id",
    );
    let mut count_state = SqlxWhereState::default();
    apply_books_filters_sqlx(
        &mut count_builder,
        &mut count_state,
        context,
        allowed.as_ref(),
        requested_series_ids,
        deleted,
        oneshot,
        scoped_to_series,
        tags,
        read_statuses,
        media_profiles,
        media_statuses,
        authors,
        release_dates,
        search,
    );
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
         FROM books b \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN series_labels sl ON sl.series_id = s.id",
    );
    let mut select_state = SqlxWhereState::default();
    apply_books_filters_sqlx(
        &mut select_builder,
        &mut select_state,
        context,
        allowed.as_ref(),
        requested_series_ids,
        deleted,
        oneshot,
        scoped_to_series,
        tags,
        read_statuses,
        media_profiles,
        media_statuses,
        authors,
        release_dates,
        search,
    );
    select_builder.push(
        " GROUP BY b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, \
            b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title \
          ORDER BY ",
    );
    select_builder.push(book_order_sql(ordering));

    let (envelope_page, envelope_size) = if unpaged {
        (0, total_elements.max(1))
    } else {
        let safe_size = size.max(1);
        let offset = page.saturating_mul(safe_size);
        select_builder.push(" LIMIT ");
        select_builder.push_bind(safe_size as i64);
        select_builder.push(" OFFSET ");
        select_builder.push_bind(offset as i64);
        (page, safe_size)
    };

    let rows = select_builder
        .build_query_as::<SqlxBookListRow>()
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

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_books_filters_sqlx<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    context: &DiscoveryQueryContext,
    allowed_library_ids: Option<&Vec<String>>,
    requested_series_ids: Option<&[String]>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    scoped_to_series: bool,
    tags: Option<&[String]>,
    read_statuses: Option<&[String]>,
    media_profiles: Option<&[String]>,
    media_statuses: Option<&[String]>,
    authors: Option<&[String]>,
    release_dates: Option<&[String]>,
    search: Option<&str>,
) {
    query_filters_sqlx(
        builder,
        state,
        "b.library_id",
        allowed_library_ids,
        search,
        Some("b.title"),
        context.restrictions.as_ref(),
        "s",
    );

    if let Some(series_ids) = requested_series_ids
        && !series_ids.is_empty()
    {
        append_clause_sqlx("b.series_id IN (", builder, state);
        let mut separated = builder.separated(",");
        for series_id in series_ids {
            separated.push_bind(series_id.clone());
        }
        separated.push_unseparated(")");
    }

    if let Some(value) = deleted {
        append_bool_sqlx_filter("b.deleted", value, builder, state);
    }

    if let Some(value) = oneshot {
        append_bool_sqlx_filter("s.oneshot", value, builder, state);
    } else if !scoped_to_series {
        append_bool_sqlx_filter("s.oneshot", false, builder, state);
    }

    if let Some(tag_values) = tags
        && !tag_values.is_empty()
    {
        append_clause_sqlx(
            "EXISTS (SELECT 1 FROM book_tags bt WHERE bt.book_id = b.id AND LOWER(bt.tag) IN (",
            builder,
            state,
        );
        let mut separated = builder.separated(",");
        for value in tag_values {
            separated.push_bind(value.to_ascii_lowercase());
        }
        separated.push_unseparated("))");
    }

    append_string_set_filter_sqlx("b.read_status", read_statuses, builder, state, true);
    append_string_set_filter_sqlx("b.media_profile", media_profiles, builder, state, true);
    append_string_set_filter_sqlx("b.media_status", media_statuses, builder, state, true);

    if let Some(author_values) = authors
        && !author_values.is_empty()
    {
        append_clause_sqlx(
            "EXISTS (SELECT 1 FROM book_authors ba WHERE ba.book_id = b.id AND LOWER(ba.author) IN (",
            builder,
            state,
        );
        let mut separated = builder.separated(",");
        for value in author_values {
            separated.push_bind(value.to_ascii_lowercase());
        }
        separated.push_unseparated("))");
    }

    append_string_set_filter_sqlx("b.metadata_release_date", release_dates, builder, state, false);
}

fn append_bool_sqlx_filter<'args>(
    column: &str,
    value: bool,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    let prefix = format!("{column} = ");
    append_clause_sqlx(prefix.as_str(), builder, state);
    builder.push_bind(i64::from(value));
}

fn book_ordering_from_sorts(sorts: &[String]) -> BookOrdering {
    let Some(sort) = sorts.first() else {
        return BookOrdering::NumberSortAsc;
    };

    let property = sort.split(',').next().unwrap_or(sort).trim();
    match property {
        "metadata.title" => BookOrdering::TitleAsc,
        "createdDate" => BookOrdering::CreatedDateDesc,
        "lastModifiedDate" => BookOrdering::LastModifiedDesc,
        "metadata.releaseDate" => BookOrdering::MetadataReleaseDateDesc,
        "metadata.numberSort" => BookOrdering::NumberSortAsc,
        _ => BookOrdering::NumberSortAsc,
    }
}

fn book_order_sql(ordering: BookOrdering) -> &'static str {
    match ordering {
        BookOrdering::TitleAsc => "b.title COLLATE NOCASE ASC",
        BookOrdering::CreatedDateDesc => "b.created DESC, b.title COLLATE NOCASE ASC",
        BookOrdering::MetadataReleaseDateDesc => {
            "b.metadata_release_date DESC, b.title COLLATE NOCASE ASC"
        }
        BookOrdering::NumberSortAsc => "b.number_sort ASC, b.title COLLATE NOCASE ASC",
        BookOrdering::LastModifiedDesc => "b.last_modified DESC, b.title COLLATE NOCASE ASC",
    }
}
