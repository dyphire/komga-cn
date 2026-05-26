use komga_application::discovery::{BookReadModel, BookReadProgressReadModel, BooksBrowseRequest};
use komga_domain::discovery::{
    BookCondition, BookSort, BookValueCondition, CompositeBookCondition, DateCondition,
    DiscoveryError, DiscoveryQueryContext, FilterOperator, InclusionCondition, NumberCondition,
    PageEnvelope, ReadStatusCondition, StringCondition,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::map_sqlx_error;
use crate::parsing::{parse_csv_values, parse_metadata_authors, parse_metadata_links};
use crate::read_models::filters::{
    QueryFilterParams, SqlxWhereState, append_clause_sqlx, append_in_clause_sqlx,
    append_like_clause_sqlx, append_not_in_clause_sqlx, effective_library_ids, query_filters_sqlx,
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
    series_title_sort: String,
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
            series_title_sort: value.series_title_sort,
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
    query: &BooksBrowseRequest,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    list_books_sqlx_common(pool, context, query, book_ordering_from_sorts(&query.sort)).await
}

pub(in crate::read_models) async fn list_books_latest_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BooksBrowseRequest,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    list_books_sqlx_common(pool, context, query, BookOrdering::LastModifiedDesc).await
}

async fn list_books_sqlx_common(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &BooksBrowseRequest,
    ordering: BookOrdering,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    let page = query.page.page;
    let size = query.page.size;
    let condition = query.filter.condition.as_ref();
    let search = query.search.as_deref();
    let unpaged = query.page.unpaged;
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
               b.deleted AS deleted, '' AS file_hash, b.oneshot AS oneshot, s.title AS series_title,
               s.title AS series_title_sort
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

fn apply_book_filter_tree_sqlx(
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
    context: &DiscoveryQueryContext,
    allowed_library_ids: Option<&Vec<String>>,
    condition: Option<&BookCondition>,
    search: Option<&str>,
) {
    query_filters_sqlx(
        builder,
        state,
        &QueryFilterParams {
            library_column: "b.library_id",
            allowed_library_ids,
            search,
            search_column: Some("b.title"),
            restrictions: context.restrictions.as_ref(),
            restriction_series_alias: "s",
        },
    );

    let has_explicit_oneshot = condition.is_some_and(condition_has_oneshot);
    let scoped_to_series = condition.is_some_and(condition_has_series_id);
    if !has_explicit_oneshot && !scoped_to_series {
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

fn condition_has_series_id(condition: &BookCondition) -> bool {
    match condition {
        BookCondition::Value(BookValueCondition::SeriesId(_)) => true,
        BookCondition::Composite(c) => c.conditions.iter().any(condition_has_series_id),
        _ => false,
    }
}

fn apply_book_condition_sqlx(
    condition: &BookCondition,
    builder: &mut QueryBuilder<Sqlite>,
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
                append_clause_sqlx("(", builder, state);
                for (i, c) in conditions.iter().enumerate() {
                    if i > 0 {
                        builder.push(" OR ");
                    }
                    builder.push(
                        "b.id IN (SELECT b.id FROM books b JOIN series s ON s.id = b.series_id",
                    );
                    let mut child_state = SqlxWhereState::default();
                    apply_book_condition_sqlx(c, builder, &mut child_state);
                    state.params.extend(child_state.params);
                    builder.push(")");
                }
                builder.push(")");
            }
        },
    }
}

fn apply_book_value_condition_sqlx(
    value: &BookValueCondition,
    builder: &mut QueryBuilder<Sqlite>,
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
        BookValueCondition::ReadListId(ids) => {
            append_readlist_condition_sqlx(ids, builder, state);
        }
        BookValueCondition::Deleted(val) => {
            append_bool_sqlx_filter("b.deleted", *val, builder, state);
        }
        BookValueCondition::OneShot(val) => {
            append_bool_sqlx_filter("s.oneshot", *val, builder, state);
        }
        BookValueCondition::Title(condition) => {
            append_string_condition_sqlx("b.title", condition, builder, state);
        }
        BookValueCondition::Tag(condition) => {
            append_book_text_relation_condition_sqlx("book_tags", "tag", condition, builder, state);
        }
        BookValueCondition::ReadStatus(ReadStatusCondition::Include(v)) => {
            append_in_clause_sqlx("b.read_status", v, builder, state);
        }
        BookValueCondition::ReadStatus(ReadStatusCondition::Exclude(v)) => {
            append_string_excludes_sqlx("b.read_status", v, builder, state);
        }
        BookValueCondition::MediaProfile(InclusionCondition::Include(v)) => {
            append_in_clause_sqlx("b.media_profile", v, builder, state);
        }
        BookValueCondition::MediaProfile(InclusionCondition::Exclude(v)) => {
            append_string_excludes_sqlx("b.media_profile", v, builder, state);
        }
        BookValueCondition::MediaStatus(InclusionCondition::Include(v)) => {
            append_in_clause_sqlx("b.media_status", v, builder, state);
        }
        BookValueCondition::MediaStatus(InclusionCondition::Exclude(v)) => {
            append_string_excludes_sqlx("b.media_status", v, builder, state);
        }
        BookValueCondition::Author(condition) => {
            append_book_text_relation_condition_sqlx(
                "book_authors",
                "author",
                condition,
                builder,
                state,
            );
        }
        BookValueCondition::NumberSort(condition) => {
            append_number_condition_sqlx("b.number_sort", condition, builder, state);
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
        BookValueCondition::ReleaseDate(DateCondition::Exact(InclusionCondition::Exclude(v))) => {
            append_string_excludes_sqlx("b.metadata_release_date", v, builder, state);
        }
        BookValueCondition::ReleaseDate(DateCondition::Contains(InclusionCondition::Include(
            v,
        ))) => {
            append_string_patterns_sqlx(
                "b.metadata_release_date",
                v,
                PatternKind::Contains,
                builder,
                state,
            );
        }
        BookValueCondition::ReleaseDate(DateCondition::Contains(InclusionCondition::Exclude(
            v,
        ))) => {
            append_string_pattern_excludes_sqlx(
                "b.metadata_release_date",
                v,
                PatternKind::Contains,
                builder,
                state,
            );
        }
        BookValueCondition::ReleaseDate(DateCondition::StartsWith(
            InclusionCondition::Include(v),
        )) => {
            append_string_patterns_sqlx(
                "b.metadata_release_date",
                v,
                PatternKind::StartsWith,
                builder,
                state,
            );
        }
        BookValueCondition::ReleaseDate(DateCondition::StartsWith(
            InclusionCondition::Exclude(v),
        )) => {
            append_string_pattern_excludes_sqlx(
                "b.metadata_release_date",
                v,
                PatternKind::StartsWith,
                builder,
                state,
            );
        }
        BookValueCondition::ReleaseDate(DateCondition::EndsWith(InclusionCondition::Include(
            v,
        ))) => {
            append_string_patterns_sqlx(
                "b.metadata_release_date",
                v,
                PatternKind::EndsWith,
                builder,
                state,
            );
        }
        BookValueCondition::ReleaseDate(DateCondition::EndsWith(InclusionCondition::Exclude(
            v,
        ))) => {
            append_string_pattern_excludes_sqlx(
                "b.metadata_release_date",
                v,
                PatternKind::EndsWith,
                builder,
                state,
            );
        }
        BookValueCondition::ReleaseDate(DateCondition::IsEmpty) => {
            append_clause_sqlx(
                "(b.metadata_release_date IS NULL OR b.metadata_release_date = '')",
                builder,
                state,
            );
        }
        BookValueCondition::ReleaseDate(DateCondition::IsNotEmpty) => {
            append_clause_sqlx(
                "(b.metadata_release_date IS NOT NULL AND b.metadata_release_date != '')",
                builder,
                state,
            );
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum PatternKind {
    Contains,
    StartsWith,
    EndsWith,
}

fn append_readlist_condition_sqlx(
    condition: &InclusionCondition<komga_domain::common_ids::ReadListId>,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    let (include, ids) = match condition {
        InclusionCondition::Include(ids) => (true, ids),
        InclusionCondition::Exclude(ids) => (false, ids),
    };
    if ids.is_empty() {
        return;
    }

    if include {
        append_clause_sqlx(
            "EXISTS (SELECT 1 FROM readlist_books rlb WHERE rlb.book_id = b.id AND rlb.readlist_id IN (",
            builder,
            state,
        );
    } else {
        append_clause_sqlx(
            "NOT EXISTS (SELECT 1 FROM readlist_books rlb WHERE rlb.book_id = b.id AND rlb.readlist_id IN (",
            builder,
            state,
        );
    }

    let mut separated = builder.separated(",");
    for id in ids {
        separated.push_bind(id.as_str().to_string());
    }
    separated.push_unseparated("))");
}

fn append_string_condition_sqlx(
    column: &str,
    condition: &StringCondition,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    match condition {
        StringCondition::Exact(InclusionCondition::Include(v)) => {
            append_in_clause_sqlx(column, v, builder, state);
        }
        StringCondition::Exact(InclusionCondition::Exclude(v)) => {
            append_string_excludes_sqlx(column, v, builder, state);
        }
        StringCondition::Contains(InclusionCondition::Include(v)) => {
            append_string_patterns_sqlx(column, v, PatternKind::Contains, builder, state);
        }
        StringCondition::Contains(InclusionCondition::Exclude(v)) => {
            append_string_pattern_excludes_sqlx(column, v, PatternKind::Contains, builder, state);
        }
        StringCondition::StartsWith(InclusionCondition::Include(v)) => {
            append_string_patterns_sqlx(column, v, PatternKind::StartsWith, builder, state);
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(v)) => {
            append_string_pattern_excludes_sqlx(column, v, PatternKind::StartsWith, builder, state);
        }
        StringCondition::EndsWith(InclusionCondition::Include(v)) => {
            append_string_patterns_sqlx(column, v, PatternKind::EndsWith, builder, state);
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(v)) => {
            append_string_pattern_excludes_sqlx(column, v, PatternKind::EndsWith, builder, state);
        }
        StringCondition::IsEmpty => {
            append_clause_sqlx(
                &format!("({column} IS NULL OR {column} = '')"),
                builder,
                state,
            );
        }
        StringCondition::IsNotEmpty => {
            append_clause_sqlx(
                &format!("({column} IS NOT NULL AND {column} != '')"),
                builder,
                state,
            );
        }
        StringCondition::Regex(_) => {}
    }
}

fn append_string_excludes_sqlx(
    column: &str,
    values: &[String],
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    for value in values {
        append_not_in_clause_sqlx(column, value, builder, state);
    }
}

fn append_string_patterns_sqlx(
    column: &str,
    values: &[String],
    kind: PatternKind,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    for value in values {
        append_like_clause_sqlx(column, &pattern_value(value, kind), builder, state);
    }
}

fn append_string_pattern_excludes_sqlx(
    column: &str,
    values: &[String],
    kind: PatternKind,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    for value in values {
        append_clause_sqlx(&format!("{column} NOT LIKE "), builder, state);
        builder.push_bind(pattern_value(value, kind));
    }
}

fn append_book_text_relation_condition_sqlx(
    table: &str,
    value_column: &str,
    condition: &StringCondition,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    match condition {
        StringCondition::Exact(InclusionCondition::Include(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                true,
                None,
                builder,
                state,
            );
        }
        StringCondition::Exact(InclusionCondition::Exclude(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                false,
                None,
                builder,
                state,
            );
        }
        StringCondition::Contains(InclusionCondition::Include(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                true,
                Some(PatternKind::Contains),
                builder,
                state,
            );
        }
        StringCondition::Contains(InclusionCondition::Exclude(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                false,
                Some(PatternKind::Contains),
                builder,
                state,
            );
        }
        StringCondition::StartsWith(InclusionCondition::Include(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                true,
                Some(PatternKind::StartsWith),
                builder,
                state,
            );
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                false,
                Some(PatternKind::StartsWith),
                builder,
                state,
            );
        }
        StringCondition::EndsWith(InclusionCondition::Include(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                true,
                Some(PatternKind::EndsWith),
                builder,
                state,
            );
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(values)) => {
            append_book_text_relation_values_sqlx(
                table,
                value_column,
                values,
                false,
                Some(PatternKind::EndsWith),
                builder,
                state,
            );
        }
        StringCondition::IsEmpty => {
            append_clause_sqlx(
                &format!(
                    "NOT EXISTS (SELECT 1 FROM {table} f WHERE f.book_id = b.id AND f.{value_column} != '')"
                ),
                builder,
                state,
            );
        }
        StringCondition::IsNotEmpty => {
            append_clause_sqlx(
                &format!(
                    "EXISTS (SELECT 1 FROM {table} f WHERE f.book_id = b.id AND f.{value_column} != '')"
                ),
                builder,
                state,
            );
        }
        StringCondition::Regex(_) => {}
    }
}

fn append_book_text_relation_values_sqlx(
    table: &str,
    value_column: &str,
    values: &[String],
    include: bool,
    pattern: Option<PatternKind>,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    if values.is_empty() {
        return;
    }

    if include {
        append_clause_sqlx(
            &format!("EXISTS (SELECT 1 FROM {table} f WHERE f.book_id = b.id AND "),
            builder,
            state,
        );
    } else {
        append_clause_sqlx(
            &format!("NOT EXISTS (SELECT 1 FROM {table} f WHERE f.book_id = b.id AND "),
            builder,
            state,
        );
    }

    match pattern {
        Some(kind) => {
            builder.push("(");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    builder.push(" OR ");
                }
                builder.push(format!("LOWER(f.{value_column}) LIKE "));
                builder.push_bind(pattern_value(&value.to_ascii_lowercase(), kind));
            }
            builder.push("))");
        }
        None => {
            builder.push(format!("LOWER(f.{value_column}) IN ("));
            let mut separated = builder.separated(",");
            for value in values {
                separated.push_bind(value.to_ascii_lowercase());
            }
            separated.push_unseparated("))");
        }
    }
}

fn append_number_condition_sqlx(
    column: &str,
    condition: &NumberCondition,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    match condition {
        NumberCondition::Exact(InclusionCondition::Include(values)) => {
            append_number_values_sqlx(column, values, true, builder, state);
        }
        NumberCondition::Exact(InclusionCondition::Exclude(values)) => {
            append_number_values_sqlx(column, values, false, builder, state);
        }
        NumberCondition::GreaterThan(value) => {
            if let Ok(number) = value.parse::<f64>() {
                append_clause_sqlx(&format!("{column} > "), builder, state);
                builder.push_bind(number);
            }
        }
        NumberCondition::LessThan(value) => {
            if let Ok(number) = value.parse::<f64>() {
                append_clause_sqlx(&format!("{column} < "), builder, state);
                builder.push_bind(number);
            }
        }
    }
}

fn append_number_values_sqlx(
    column: &str,
    values: &[String],
    include: bool,
    builder: &mut QueryBuilder<Sqlite>,
    state: &mut SqlxWhereState,
) {
    let parsed = values
        .iter()
        .filter_map(|value| value.parse::<f64>().ok())
        .collect::<Vec<_>>();
    if parsed.is_empty() {
        return;
    }

    if parsed.len() == 1 {
        append_clause_sqlx(
            &format!("{column} {} ", if include { "=" } else { "!=" }),
            builder,
            state,
        );
        builder.push_bind(parsed[0]);
        return;
    }

    append_clause_sqlx(
        &format!("{column} {} (", if include { "IN" } else { "NOT IN" }),
        builder,
        state,
    );
    let mut separated = builder.separated(",");
    for number in parsed {
        separated.push_bind(number);
    }
    separated.push_unseparated(")");
}

fn pattern_value(value: &str, kind: PatternKind) -> String {
    match kind {
        PatternKind::Contains => format!("%{value}%"),
        PatternKind::StartsWith => format!("{value}%"),
        PatternKind::EndsWith => format!("%{value}"),
    }
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

fn append_bool_sqlx_filter(
    column: &str,
    value: bool,
    builder: &mut QueryBuilder<Sqlite>,
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
