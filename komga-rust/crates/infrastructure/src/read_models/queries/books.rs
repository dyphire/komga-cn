use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadModel,
    BookReadProgressReadModel,
};
use komga_application::discovery::{RuntimeBooksLatestQuery, RuntimeBooksListQuery};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, PageEnvelope};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::map_sqlx_error;
use crate::read_models::filters::{
    SqlxWhereState, append_clause_sqlx, append_string_set_filter_sqlx, effective_library_ids,
    query_filters_sqlx,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BookOrdering {
    TitleAsc,
    CreatedDateDesc,
    MetadataReleaseDateDesc,
    NumberSortAsc,
    SeriesIdAsc,
    LastModifiedDesc,
}

#[derive(sqlx::FromRow)]
struct SqlxBookListRow {
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

impl From<SqlxBookListRow> for BookReadModel {
    fn from(value: SqlxBookListRow) -> Self {
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
            metadata_authors: parse_metadata_authors(&value.metadata_authors),
            metadata_authors_lock: value.metadata_authors_lock,
            metadata_tags: parse_csv_values(&value.metadata_tags),
            metadata_tags_lock: value.metadata_tags_lock,
            metadata_isbn: value.metadata_isbn,
            metadata_isbn_lock: value.metadata_isbn_lock,
            metadata_links: parse_metadata_links(&value.metadata_links),
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

pub(in crate::read_models) async fn list_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeBooksListQuery,
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

pub(in crate::read_models) async fn list_books_latest_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeBooksLatestQuery,
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
        "SELECT COUNT(DISTINCT b.id) \
         FROM books b \
         JOIN series s ON s.id = b.series_id",
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
        "SELECT b.id AS id, b.series_id AS series_id, b.library_id AS library_id, \
                b.title AS title, b.url AS url, CAST(b.number_sort AS INTEGER) AS number, \
                b.created AS created, b.last_modified AS last_modified, \
                b.file_last_modified AS file_last_modified, b.size_bytes AS size_bytes, \
                COALESCE(b.media_status, 'UNKNOWN') AS media_status, \
                COALESCE(b.media_type, '') AS media_type, \
                COALESCE(b.media_pages_count, 0) AS media_pages_count, \
                '' AS media_comment, 0 AS media_epub_divina_compatible, 0 AS media_epub_is_kepub, \
                COALESCE(b.metadata_release_date, NULL) AS metadata_release_date, \
                0 AS metadata_title_lock, '' AS metadata_summary, 0 AS metadata_summary_lock, \
                CAST(b.number_sort AS TEXT) AS metadata_number, 0 AS metadata_number_lock, \
                CAST(b.number_sort AS REAL) AS metadata_number_sort, 0 AS metadata_number_sort_lock, \
                0 AS metadata_release_date_lock, \
                COALESCE((SELECT GROUP_CONCAT(ba.author, X'1F') FROM book_authors ba WHERE ba.book_id = b.id), '') AS metadata_authors, \
                0 AS metadata_authors_lock, \
                COALESCE((SELECT GROUP_CONCAT(bt.tag) FROM book_tags bt WHERE bt.book_id = b.id), '') AS metadata_tags, \
                0 AS metadata_tags_lock, '' AS metadata_isbn, 0 AS metadata_isbn_lock, \
                '' AS metadata_links, 0 AS metadata_links_lock, \
                b.created AS metadata_created, b.last_modified AS metadata_last_modified, \
                rp.page AS read_progress_page, rp.completed AS read_progress_completed, \
                rp.read_date AS read_progress_read_date, rp.created AS read_progress_created, \
                rp.last_modified AS read_progress_last_modified, rp.device_id AS read_progress_device_id, \
                rp.device_name AS read_progress_device_name, \
                b.deleted AS deleted, '' AS file_hash, b.oneshot AS oneshot, s.title AS series_title \
         FROM books b \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN read_progress rp ON rp.book_id = b.id AND rp.user_id = ",
    );
    let user_id = context
        .user_id
        .as_ref()
        .map(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    select_builder.push_bind(user_id);
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
    select_builder.push(" ORDER BY ");
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

pub(super) fn parse_csv_values(raw: &str) -> Vec<String> {
    raw.split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_string())
        .collect()
}

pub(super) fn parse_metadata_authors(raw: &str) -> Vec<BookMetadataAuthorReadModel> {
    raw.split('\u{001F}')
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.split_once('\u{001E}') {
            Some((name, role)) => BookMetadataAuthorReadModel {
                name: name.to_string(),
                role: role.to_string(),
            },
            None => BookMetadataAuthorReadModel {
                name: entry.to_string(),
                role: String::new(),
            },
        })
        .collect()
}

pub(super) fn parse_metadata_links(raw: &str) -> Vec<BookMetadataLinkReadModel> {
    raw.split('\u{001F}')
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            entry
                .split_once('\u{001E}')
                .map(|(label, url)| BookMetadataLinkReadModel {
                    label: label.to_string(),
                    url: url.to_string(),
                })
        })
        .collect()
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

    append_string_set_filter_sqlx(
        "b.metadata_release_date",
        release_dates,
        builder,
        state,
        false,
    );
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

    match sort.as_str() {
        "metadata.title,asc" | "metadata.title" | "title,asc" | "title" => BookOrdering::TitleAsc,
        "createdDate,desc" | "created,desc" | "createdDate" | "created" => {
            BookOrdering::CreatedDateDesc
        }
        "lastModifiedDate,desc" | "lastModified,desc" | "lastModifiedDate" | "lastModified" => {
            BookOrdering::LastModifiedDesc
        }
        "metadata.releaseDate,desc" | "metadata.releaseDate" => {
            BookOrdering::MetadataReleaseDateDesc
        }
        "series,metadata.numberSort,asc"
        | "metadata.numberSort,asc"
        | "metadata.numberSort"
        | "number,asc"
        | "number" => BookOrdering::NumberSortAsc,
        "seriesId,asc" | "seriesId" => BookOrdering::SeriesIdAsc,
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
        BookOrdering::SeriesIdAsc => {
            "b.series_id ASC, b.number_sort ASC, b.title COLLATE NOCASE ASC"
        }
        BookOrdering::LastModifiedDesc => "b.last_modified DESC, b.title COLLATE NOCASE ASC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_ordering_from_sorts_supports_runtime_aliases() {
        assert_eq!(
            book_ordering_from_sorts(&["metadata.title,asc".to_string()]),
            BookOrdering::TitleAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&["created,desc".to_string()]),
            BookOrdering::CreatedDateDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&["lastModified,desc".to_string()]),
            BookOrdering::LastModifiedDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&["metadata.releaseDate,desc".to_string()]),
            BookOrdering::MetadataReleaseDateDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&["series,metadata.numberSort,asc".to_string()]),
            BookOrdering::NumberSortAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&["metadata.numberSort,asc".to_string()]),
            BookOrdering::NumberSortAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&["number,asc".to_string()]),
            BookOrdering::NumberSortAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&["seriesId,asc".to_string()]),
            BookOrdering::SeriesIdAsc
        );
    }
}
