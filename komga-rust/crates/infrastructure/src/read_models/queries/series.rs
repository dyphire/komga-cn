use komga_application::discovery::{RuntimeSeriesListQuery, SeriesDetailQuery};
use komga_application::discovery::{
    SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, PageEnvelope};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::super::filters::{
    SqlValue, SqlxWhereState, append_clause_sqlx, append_exists_series_filter_sqlx,
    append_in_clause_sqlx, append_string_set_filter_sqlx, append_u16_set_filter_sqlx,
    apply_restrictions_sqlx, effective_library_ids, query_filters_sqlx,
};
use super::map_sqlx_error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeriesOrdering {
    TitleAsc,
    CreatedDateDesc,
    LastModifiedDateDesc,
    BooksMetadataReleaseDateDesc,
    BooksCountDesc,
}

#[derive(sqlx::FromRow)]
struct SqlxSeriesResourceRow {
    id: String,
    url: String,
}

impl From<SqlxSeriesResourceRow> for SeriesResourceReadModel {
    fn from(value: SqlxSeriesResourceRow) -> Self {
        Self {
            id: value.id,
            url: value.url,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxSeriesListRow {
    id: String,
    title: String,
}

impl From<SqlxSeriesListRow> for SeriesReadModel {
    fn from(value: SqlxSeriesListRow) -> Self {
        Self {
            id: value.id,
            title: value.title,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxSeriesDetailRow {
    id: String,
    title: String,
}

impl From<SqlxSeriesDetailRow> for SeriesDetailReadModel {
    fn from(value: SqlxSeriesDetailRow) -> Self {
        Self {
            id: value.id,
            title: value.title,
        }
    }
}

pub(in crate::read_models) async fn list_series_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeSeriesListQuery,
) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, query.library_ids.as_deref());
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(
            vec![],
            query.page,
            query.size.max(1),
            0,
        ));
    }

    let mut count_builder = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(DISTINCT s.id) \
                                     FROM series s",
    );
    let mut count_state = SqlxWhereState::default();
    apply_series_list_filters_sqlx(
        &mut count_builder,
        &mut count_state,
        context,
        query,
        allowed.as_ref(),
    );
    let total_elements = count_builder
        .build_query_scalar::<i64>()
        .fetch_one(&pool)
        .await
        .map_err(map_sqlx_error)? as usize;

    let safe_size = query.size.max(1);
    let offset = query.page.saturating_mul(safe_size);

    let mut select_builder = QueryBuilder::<Sqlite>::new(
        "SELECT s.id AS id, s.library_id AS library_id, s.title AS title, \
                s.created AS created, s.last_modified AS last_modified, \
                s.release_date AS release_date, s.book_count AS book_count, \
                COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS labels \
         FROM series s \
         LEFT \
         JOIN series_labels sl ON sl.series_id = s.id",
    );
    let mut select_state = SqlxWhereState::default();
    apply_series_list_filters_sqlx(
        &mut select_builder,
        &mut select_state,
        context,
        query,
        allowed.as_ref(),
    );
    select_builder.push(
        " GROUP BY s.id, s.library_id, s.title, s.created, s.last_modified, s.release_date, s.book_count ORDER BY ",
    );
    select_builder.push(series_order_sql(series_ordering_from_sorts(&query.sort)));
    select_builder.push(" LIMIT ");
    select_builder.push_bind(safe_size as i64);
    select_builder.push(" OFFSET ");
    select_builder.push_bind(offset as i64);

    let rows = select_builder
        .build_query_as::<SqlxSeriesListRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;

    Ok(PageEnvelope::from_slice(
        rows.into_iter().map(SeriesReadModel::from).collect(),
        query.page,
        safe_size,
        total_elements,
    ))
}

fn series_ordering_from_sorts(sorts: &[String]) -> SeriesOrdering {
    let Some(sort) = sorts.first() else {
        return SeriesOrdering::TitleAsc;
    };

    match sort.as_str() {
        "metadata.titleSort,asc" | "titleSort,asc" | "metadata.titleSort" | "titleSort" => {
            SeriesOrdering::TitleAsc
        }
        "createdDate,desc" | "created,desc" | "createdDate" | "created" => {
            SeriesOrdering::CreatedDateDesc
        }
        "lastModifiedDate,desc"
        | "lastModified,desc"
        | "lastModifiedDate"
        | "lastModified" => SeriesOrdering::LastModifiedDateDesc,
        "booksMetadata.releaseDate,desc" | "booksMetadata.releaseDate" => {
            SeriesOrdering::BooksMetadataReleaseDateDesc
        }
        "booksCount,desc" | "booksCount" => SeriesOrdering::BooksCountDesc,
        _ => SeriesOrdering::TitleAsc,
    }
}

fn series_order_sql(ordering: SeriesOrdering) -> &'static str {
    match ordering {
        SeriesOrdering::TitleAsc => "s.title COLLATE NOCASE ASC",
        SeriesOrdering::CreatedDateDesc => "s.created DESC, s.title COLLATE NOCASE ASC",
        SeriesOrdering::LastModifiedDateDesc => {
            "s.last_modified DESC, s.title COLLATE NOCASE ASC"
        }
        SeriesOrdering::BooksMetadataReleaseDateDesc => {
            "s.release_date DESC, s.title COLLATE NOCASE ASC"
        }
        SeriesOrdering::BooksCountDesc => "s.book_count DESC, s.title COLLATE NOCASE ASC",
    }
}

pub(in crate::read_models) async fn get_series_detail_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &SeriesDetailQuery,
) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT s.id AS id, s.library_id AS library_id, s.title AS title, s.url AS url, \
                s.created AS created, s.last_modified AS last_modified, \
                s.file_last_modified AS file_last_modified, s.status AS status, \
                s.publisher AS publisher, s.age_rating AS age_rating, s.language AS language, \
                s.deleted AS deleted, s.oneshot AS oneshot, \
                COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS sharing_labels, \
                COALESCE(GROUP_CONCAT(DISTINCT sg.genre), '') AS genres, \
                COALESCE(GROUP_CONCAT(DISTINCT st.tag), '') AS tags, COALESCE((SELECT COUNT(*) \
         FROM books b \
         WHERE b.series_id = s.id), 0) AS books_count, COALESCE((SELECT COUNT(*) \
         FROM books b \
         WHERE b.series_id = s.id \
         AND LOWER(b.read_status) = 'read'), 0) AS books_read_count, COALESCE((SELECT COUNT(*) \
         FROM books b \
         WHERE b.series_id = s.id \
         AND LOWER(b.read_status) = 'unread'), 0) AS books_unread_count, \
           COALESCE((SELECT COUNT(*) \
         FROM books b \
         WHERE b.series_id = s.id \
         AND LOWER(b.read_status) = 'in_progress'), 0) AS books_in_progress_count, \
           COALESCE((SELECT MIN(b.metadata_release_date) \
         FROM books b \
         WHERE b.series_id = s.id), NULL) AS books_metadata_release_date \
         FROM series s \
         LEFT \
         JOIN series_labels sl ON sl.series_id = s.id \
         LEFT \
         JOIN series_genres sg ON sg.series_id = s.id \
         LEFT \
         JOIN series_tags st ON st.series_id = s.id",
    );
    let mut state = SqlxWhereState::default();

    append_clause_sqlx("s.id = ", &mut builder, &mut state);
    builder.push_bind(query.series_id.clone());
    state.params.push(SqlValue::Text(query.series_id.clone()));

    if let Some(allowed_ids) = allowed.as_ref() {
        append_in_clause_sqlx("s.library_id", allowed_ids, &mut builder, &mut state);
    }

    if let Some(restrictions) = context.restrictions.as_ref() {
        apply_restrictions_sqlx("s", restrictions, &mut builder, &mut state);
    }

    builder.push(
        " GROUP BY s.id, s.library_id, s.title, s.url, s.created, s.last_modified, s.file_last_modified, \
           s.status, s.publisher, s.age_rating, s.language, s.deleted, s.oneshot",
    );

    let row = builder
        .build_query_as::<SqlxSeriesDetailRow>()
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;

    Ok(row.map(SeriesDetailReadModel::from))
}

pub(in crate::read_models) async fn resolve_series_resource_sqlx(
    pool: SqlitePool,
    series_id: &str,
) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
    let row = sqlx::query_as::<_, SqlxSeriesResourceRow>(
        "SELECT s.id, s.url \
         FROM series s \
         WHERE s.id = ?",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(row.map(SeriesResourceReadModel::from))
}

fn apply_series_list_filters_sqlx<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    context: &DiscoveryQueryContext,
    query: &RuntimeSeriesListQuery,
    allowed_library_ids: Option<&Vec<String>>,
) {
    query_filters_sqlx(
        builder,
        state,
        "s.library_id",
        allowed_library_ids,
        query.search.as_deref(),
        Some("s.title"),
        context.restrictions.as_ref(),
        "s",
    );

    if let Some(deleted) = query.deleted {
        append_bool_sqlx_filter("s.deleted", deleted, builder, state);
    }
    if let Some(oneshot) = query.oneshot {
        append_bool_sqlx_filter("s.oneshot", oneshot, builder, state);
    }

    append_string_set_filter_sqlx(
        "s.read_status",
        query.read_statuses.as_deref(),
        builder,
        state,
        true,
    );
    append_exists_series_filter_sqlx(
        "series_genres",
        "genre",
        query.genres.as_deref(),
        builder,
        state,
    );
    append_exists_series_filter_sqlx("series_tags", "tag", query.tags.as_deref(), builder, state);
    append_string_set_filter_sqlx(
        "s.language",
        query.languages.as_deref(),
        builder,
        state,
        true,
    );
    append_string_set_filter_sqlx(
        "s.publisher",
        query.publishers.as_deref(),
        builder,
        state,
        true,
    );
    append_u16_set_filter_sqlx("s.age_rating", query.age_ratings.as_deref(), builder, state);
    append_string_set_filter_sqlx(
        "s.release_date",
        query.release_dates.as_deref(),
        builder,
        state,
        false,
    );
    append_exists_series_filter_sqlx(
        "series_labels",
        "label",
        query.sharing_labels.as_deref(),
        builder,
        state,
    );
    append_string_set_filter_sqlx(
        "s.status",
        query.series_statuses.as_deref(),
        builder,
        state,
        true,
    );
    if let Some(complete) = query.complete {
        append_bool_sqlx_filter("s.complete", complete, builder, state);
    }
    append_exists_series_filter_sqlx(
        "series_authors",
        "author",
        query.authors.as_deref(),
        builder,
        state,
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
    state.params.push(SqlValue::Integer(i64::from(value)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_ordering_from_sorts_supports_runtime_aliases() {
        assert_eq!(
            series_ordering_from_sorts(&["metadata.titleSort,asc".to_string()]),
            SeriesOrdering::TitleAsc
        );
        assert_eq!(
            series_ordering_from_sorts(&["titleSort,asc".to_string()]),
            SeriesOrdering::TitleAsc
        );
        assert_eq!(
            series_ordering_from_sorts(&["created,desc".to_string()]),
            SeriesOrdering::CreatedDateDesc
        );
        assert_eq!(
            series_ordering_from_sorts(&["lastModified,desc".to_string()]),
            SeriesOrdering::LastModifiedDateDesc
        );
        assert_eq!(
            series_ordering_from_sorts(&["booksMetadata.releaseDate,desc".to_string()]),
            SeriesOrdering::BooksMetadataReleaseDateDesc
        );
        assert_eq!(
            series_ordering_from_sorts(&["booksCount,desc".to_string()]),
            SeriesOrdering::BooksCountDesc
        );
    }
}
