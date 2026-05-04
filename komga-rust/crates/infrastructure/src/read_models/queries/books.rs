use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadModel,
    BookReadProgressReadModel, BooksBrowseQuery,
};
use komga_domain::discovery::{
    BookCondition, BookSort, BookValueCondition, CompositeBookCondition, DateCondition,
    DiscoveryError, DiscoveryQueryContext, FilterOperator, InclusionCondition, NumberCondition,
    PageEnvelope, ReadStatusCondition, StringCondition,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::map_sqlx_error;
use crate::read_models::filters::{
    SqlxWhereState, append_clause_sqlx, append_in_clause_sqlx, append_like_clause_sqlx,
    append_not_in_clause_sqlx, append_subquery_exists_clause, effective_library_ids,
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
    ReadProgressLastModifiedAsc,
    ReadProgressLastModifiedDesc,
    ReadProgressReadDateAsc,
    ReadProgressReadDateDesc,
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
    query: &BooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    list_books_sqlx_common(
        pool,
        context,
        query.page,
        query.size,
        query.filter.condition.as_ref(),
        query.search.as_deref(),
        query.unpaged,
        book_ordering_from_sorts(&query.sort),
    )
    .await
}

pub(in crate::read_models) async fn list_books_latest_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    list_books_sqlx_common(
        pool,
        context,
        query.page,
        query.size,
        query.filter.condition.as_ref(),
        query.search.as_deref(),
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
    condition: Option<&BookCondition>,
    search: Option<&str>,
    unpaged: bool,
    ordering: BookOrdering,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    let requested_library_ids = extract_book_library_ids(condition);
    let allowed = effective_library_ids(context, requested_library_ids.as_deref());
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(vec![], page, size.max(1), 0));
    }

    let mut count_builder = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT COUNT(DISTINCT b.id)
        FROM books b
        JOIN series s ON s.id = b.series_id
        "#,
    );
    let mut count_state = SqlxWhereState::default();
    apply_book_filter_tree_sqlx(
        &mut count_builder,
        &mut count_state,
        context,
        allowed.as_ref(),
        condition,
        search,
    );
    let total_elements = count_builder
        .build_query_scalar::<i64>()
        .fetch_one(&pool)
        .await
        .map_err(map_sqlx_error)? as usize;

    let mut select_builder = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT b.id AS id, b.series_id AS series_id, b.library_id AS library_id,
               b.title AS title, b.url AS url, CAST(b.number_sort AS INTEGER) AS number,
               b.created AS created, b.last_modified AS last_modified,
               b.file_last_modified AS file_last_modified, b.size_bytes AS size_bytes,
               COALESCE(b.media_status, 'UNKNOWN') AS media_status,
               COALESCE(b.media_type, '') AS media_type,
               COALESCE(b.media_pages_count, 0) AS media_pages_count,
               '' AS media_comment, 0 AS media_epub_divina_compatible, 0 AS media_epub_is_kepub,
               COALESCE(b.metadata_release_date, NULL) AS metadata_release_date,
               0 AS metadata_title_lock, '' AS metadata_summary, 0 AS metadata_summary_lock,
               CAST(b.number_sort AS TEXT) AS metadata_number, 0 AS metadata_number_lock,
               CAST(b.number_sort AS REAL) AS metadata_number_sort, 0 AS metadata_number_sort_lock,
               0 AS metadata_release_date_lock,
               COALESCE((SELECT GROUP_CONCAT(ba.author, X'1F') FROM book_authors ba WHERE ba.book_id = b.id), '') AS metadata_authors,
               0 AS metadata_authors_lock,
               COALESCE((SELECT GROUP_CONCAT(bt.tag) FROM book_tags bt WHERE bt.book_id = b.id), '') AS metadata_tags,
               0 AS metadata_tags_lock, '' AS metadata_isbn, 0 AS metadata_isbn_lock,
               '' AS metadata_links, 0 AS metadata_links_lock,
               b.created AS metadata_created, b.last_modified AS metadata_last_modified,
               rp.page AS read_progress_page, rp.completed AS read_progress_completed,
               rp.read_date AS read_progress_read_date, rp.created AS read_progress_created,
               rp.last_modified AS read_progress_last_modified, rp.device_id AS read_progress_device_id,
               rp.device_name AS read_progress_device_name,
               b.deleted AS deleted, '' AS file_hash, b.oneshot AS oneshot, s.title AS series_title
        FROM books b
        JOIN series s ON s.id = b.series_id
        LEFT JOIN read_progress rp ON rp.book_id = b.id AND rp.user_id =
        "#,
    );
    let user_id = context
        .user_id
        .as_ref()
        .map(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    select_builder.push_bind(user_id);
    let mut select_state = SqlxWhereState::default();
    apply_book_filter_tree_sqlx(
        &mut select_builder,
        &mut select_state,
        context,
        allowed.as_ref(),
        condition,
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

fn apply_book_filter_tree_sqlx<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    context: &DiscoveryQueryContext,
    allowed_library_ids: Option<&Vec<String>>,
    condition: Option<&BookCondition>,
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

    let has_explicit_oneshot = condition.is_some_and(condition_has_oneshot);
    if !has_explicit_oneshot {
        append_bool_sqlx_filter("s.oneshot", false, builder, state);
    }

    if let Some(condition) = condition {
        apply_book_condition_sqlx(condition, builder, state);
    }
}

fn condition_has_oneshot(condition: &BookCondition) -> bool {
    match condition {
        BookCondition::Value(BookValueCondition::OneShot(_)) => true,
        BookCondition::Composite(c) => c.conditions.iter().any(condition_has_oneshot),
        _ => false,
    }
}

fn apply_book_condition_sqlx<'args>(
    condition: &BookCondition,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    match condition {
        BookCondition::Value(value) => apply_book_value_condition_sqlx(value, builder, state),
        BookCondition::Composite(CompositeBookCondition {
            operator,
            conditions,
        }) => match operator {
            FilterOperator::All => {
                for c in conditions {
                    apply_book_condition_sqlx(c, builder, state);
                }
            }
            FilterOperator::Any => {
                if conditions.is_empty() {
                    return;
                }
                let prefix = if state.has_where {
                    " AND ("
                } else {
                    " WHERE ("
                };
                builder.push(prefix);
                for (i, c) in conditions.iter().enumerate() {
                    if i > 0 {
                        builder.push(" OR ");
                    }
                    apply_book_condition_sqlx(c, builder, state);
                }
                builder.push(")");
                state.has_where = true;
            }
        },
    }
}

fn apply_book_value_condition_sqlx<'args>(
    value: &BookValueCondition,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    match value {
        BookValueCondition::LibraryId(InclusionCondition::Include(ids)) => {
            let str_ids: Vec<String> = ids.iter().map(|id| id.as_str().to_string()).collect();
            append_in_clause_sqlx("b.library_id", &str_ids, builder, state);
        }
        BookValueCondition::LibraryId(InclusionCondition::Exclude(ids)) => {
            for id in ids {
                append_not_in_clause_sqlx("b.library_id", id.as_str(), builder, state);
            }
        }
        BookValueCondition::SeriesId(InclusionCondition::Include(ids)) => {
            let str_ids: Vec<String> = ids.iter().map(|id| id.as_str().to_string()).collect();
            append_in_clause_sqlx("b.series_id", &str_ids, builder, state);
        }
        BookValueCondition::SeriesId(InclusionCondition::Exclude(ids)) => {
            for id in ids {
                append_not_in_clause_sqlx("b.series_id", id.as_str(), builder, state);
            }
        }
        BookValueCondition::Deleted(val) => {
            append_bool_sqlx_filter("b.deleted", *val, builder, state);
        }
        BookValueCondition::OneShot(val) => {
            append_bool_sqlx_filter("s.oneshot", *val, builder, state);
        }
        BookValueCondition::Title(StringCondition::Contains(InclusionCondition::Include(v))) => {
            for val in v {
                append_like_clause_sqlx("b.title", &format!("%{val}%"), builder, state);
            }
        }
        BookValueCondition::Title(StringCondition::Exact(InclusionCondition::Include(v))) => {
            append_in_clause_sqlx("b.title", v, builder, state);
        }
        BookValueCondition::Title(StringCondition::StartsWith(InclusionCondition::Include(v))) => {
            for val in v {
                append_like_clause_sqlx("b.title", &format!("{val}%"), builder, state);
            }
        }
        BookValueCondition::Title(StringCondition::EndsWith(InclusionCondition::Include(v))) => {
            for val in v {
                append_like_clause_sqlx("b.title", &format!("%{val}"), builder, state);
            }
        }
        BookValueCondition::Tag(StringCondition::Contains(InclusionCondition::Include(v)))
        | BookValueCondition::Tag(StringCondition::Exact(InclusionCondition::Include(v))) => {
            append_subquery_exists_clause("book_tags", "book_id", "tag", v, builder, state);
        }
        BookValueCondition::ReadStatus(ReadStatusCondition::Include(v)) => {
            append_in_clause_sqlx("b.read_status", v, builder, state);
        }
        BookValueCondition::MediaProfile(InclusionCondition::Include(v)) => {
            append_in_clause_sqlx("b.media_profile", v, builder, state);
        }
        BookValueCondition::MediaStatus(InclusionCondition::Include(v)) => {
            append_in_clause_sqlx("b.media_status", v, builder, state);
        }
        BookValueCondition::Author(StringCondition::Contains(InclusionCondition::Include(v)))
        | BookValueCondition::Author(StringCondition::Exact(InclusionCondition::Include(v))) => {
            append_subquery_exists_clause("book_authors", "book_id", "author", v, builder, state);
        }
        BookValueCondition::NumberSort(NumberCondition::Exact(InclusionCondition::Include(v))) => {
            if v.len() == 1 {
                if let Ok(num) = v[0].parse::<f64>() {
                    append_clause_sqlx("b.number_sort = ", builder, state);
                    builder.push_bind(num);
                }
            } else if !v.is_empty() {
                append_clause_sqlx("b.number_sort IN (", builder, state);
                let mut separated = builder.separated(",");
                for val in v {
                    if let Ok(num) = val.parse::<f64>() {
                        separated.push_bind(num);
                    }
                }
                separated.push_unseparated(")");
            }
        }
        BookValueCondition::ReleaseDate(DateCondition::After(v)) => {
            append_clause_sqlx("b.metadata_release_date > ", builder, state);
            builder.push_bind(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::Before(v)) => {
            append_clause_sqlx("b.metadata_release_date < ", builder, state);
            builder.push_bind(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::Exact(InclusionCondition::Include(v))) => {
            append_in_clause_sqlx("b.metadata_release_date", v, builder, state);
        }
        _ => {}
    }
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

fn extract_book_library_ids(condition: Option<&BookCondition>) -> Option<Vec<String>> {
    let condition = condition?;
    let mut ids = Vec::new();
    collect_book_library_ids(condition, &mut ids);
    if ids.is_empty() { None } else { Some(ids) }
}

fn collect_book_library_ids(condition: &BookCondition, out: &mut Vec<String>) {
    match condition {
        BookCondition::Value(BookValueCondition::LibraryId(InclusionCondition::Include(
            id_list,
        ))) => {
            for id in id_list {
                out.push(id.as_str().to_string());
            }
        }
        BookCondition::Composite(c) => {
            for child in &c.conditions {
                collect_book_library_ids(child, out);
            }
        }
        _ => {}
    }
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

fn book_ordering_from_sorts(sorts: &[BookSort]) -> BookOrdering {
    match sorts.first() {
        Some(BookSort::MetadataTitleAsc) | Some(BookSort::MetadataTitleDesc) => {
            BookOrdering::TitleAsc
        }
        Some(BookSort::CreatedDateDesc) | Some(BookSort::CreatedDateAsc) => {
            BookOrdering::CreatedDateDesc
        }
        Some(BookSort::LastModifiedDateDesc) | Some(BookSort::LastModifiedDateAsc) => {
            BookOrdering::LastModifiedDesc
        }
        Some(BookSort::ReadProgressLastModifiedAsc) => BookOrdering::ReadProgressLastModifiedAsc,
        Some(BookSort::ReadProgressLastModifiedDesc) => BookOrdering::ReadProgressLastModifiedDesc,
        Some(BookSort::ReadProgressReadDateAsc) => BookOrdering::ReadProgressReadDateAsc,
        Some(BookSort::ReadProgressReadDateDesc) => BookOrdering::ReadProgressReadDateDesc,
        Some(BookSort::ReleaseDateDesc) | Some(BookSort::ReleaseDateAsc) => {
            BookOrdering::MetadataReleaseDateDesc
        }
        Some(BookSort::NumberSortAsc) | Some(BookSort::NumberSortDesc) => {
            BookOrdering::NumberSortAsc
        }
        Some(BookSort::SeriesIdAsc) => BookOrdering::SeriesIdAsc,
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
        BookOrdering::ReadProgressLastModifiedAsc => {
            "rp.last_modified ASC, b.title COLLATE NOCASE ASC"
        }
        BookOrdering::ReadProgressLastModifiedDesc => {
            "rp.last_modified DESC, b.title COLLATE NOCASE ASC"
        }
        BookOrdering::ReadProgressReadDateAsc => "rp.read_date ASC, b.title COLLATE NOCASE ASC",
        BookOrdering::ReadProgressReadDateDesc => "rp.read_date DESC, b.title COLLATE NOCASE ASC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_ordering_from_sorts_supports_domain_sorts() {
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::MetadataTitleAsc]),
            BookOrdering::TitleAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::CreatedDateDesc]),
            BookOrdering::CreatedDateDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::LastModifiedDateDesc]),
            BookOrdering::LastModifiedDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::ReadProgressLastModifiedAsc]),
            BookOrdering::ReadProgressLastModifiedAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::ReadProgressLastModifiedDesc]),
            BookOrdering::ReadProgressLastModifiedDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::ReadProgressReadDateAsc]),
            BookOrdering::ReadProgressReadDateAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::ReadProgressReadDateDesc]),
            BookOrdering::ReadProgressReadDateDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::ReleaseDateDesc]),
            BookOrdering::MetadataReleaseDateDesc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::NumberSortAsc]),
            BookOrdering::NumberSortAsc
        );
        assert_eq!(
            book_ordering_from_sorts(&[BookSort::SeriesIdAsc]),
            BookOrdering::SeriesIdAsc
        );
        assert_eq!(book_ordering_from_sorts(&[]), BookOrdering::NumberSortAsc);
    }
}
