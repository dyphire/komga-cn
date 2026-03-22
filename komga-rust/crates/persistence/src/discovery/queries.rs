use komga_application::discovery::{NativeSeriesListQuery, SeriesCollectionsQuery, SeriesDetailQuery};
use komga_domain::discovery::{
    BookResourceReadModel, CollectionReadModel, DiscoveryError, DiscoveryQueryContext,
    LibraryReadModel, PageEnvelope, SeriesDetailReadModel, SeriesReadModel,
    SeriesResourceReadModel,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

#[path = "queries/book_detail.rs"]
pub(super) mod book_detail;
#[path = "queries/books.rs"]
pub(super) mod books;
#[path = "queries/readlists.rs"]
pub(super) mod readlists;
use super::filters::{
    SqlValue, SqlxWhereState, append_clause_sqlx, append_exists_series_filter_sqlx,
    append_in_clause_sqlx, append_string_set_filter_sqlx, append_u16_set_filter_sqlx,
    apply_restrictions_sqlx, effective_library_ids, query_filters_sqlx,
};

#[derive(sqlx::FromRow)]
struct SqlxLibraryRow {
    id: String,
    name: String,
    root: String,
}

impl From<SqlxLibraryRow> for LibraryReadModel {
    fn from(value: SqlxLibraryRow) -> Self {
        Self {
            id: value.id,
            name: value.name,
            root: value.root,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxSeriesResourceRow {
    id: String,
    library_id: String,
    age_rating: Option<u16>,
    labels: String,
}

impl From<SqlxSeriesResourceRow> for SeriesResourceReadModel {
    fn from(value: SqlxSeriesResourceRow) -> Self {
        Self {
            id: value.id,
            library_id: value.library_id,
            age_rating: value.age_rating,
            labels: parse_labels(&value.labels),
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxSeriesListRow {
    id: String,
    library_id: String,
    title: String,
    labels: String,
}

impl From<SqlxSeriesListRow> for SeriesReadModel {
    fn from(value: SqlxSeriesListRow) -> Self {
        Self {
            id: value.id,
            library_id: value.library_id,
            title: value.title,
            labels: parse_labels(&value.labels),
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxSeriesDetailRow {
    id: String,
    library_id: String,
    title: String,
    url: String,
    created: String,
    last_modified: String,
    file_last_modified: String,
    status: String,
    publisher: String,
    age_rating: Option<u16>,
    language: String,
    deleted: bool,
    oneshot: bool,
    sharing_labels: String,
    genres: String,
    tags: String,
    books_count: i64,
    books_read_count: i64,
    books_unread_count: i64,
    books_in_progress_count: i64,
    books_metadata_release_date: Option<String>,
}

impl From<SqlxSeriesDetailRow> for SeriesDetailReadModel {
    fn from(value: SqlxSeriesDetailRow) -> Self {
        Self {
            id: value.id,
            library_id: value.library_id,
            title: value.title,
            url: value.url,
            created: value.created.clone(),
            last_modified: value.last_modified.clone(),
            file_last_modified: value.file_last_modified,
            status: value.status,
            publisher: value.publisher,
            age_rating: value.age_rating,
            language: value.language,
            deleted: value.deleted,
            oneshot: value.oneshot,
            sharing_labels: parse_labels(&value.sharing_labels),
            genres: parse_labels(&value.genres),
            tags: parse_labels(&value.tags),
            books_count: value.books_count as u32,
            books_read_count: value.books_read_count as u32,
            books_unread_count: value.books_unread_count as u32,
            books_in_progress_count: value.books_in_progress_count as u32,
            books_metadata_release_date: value.books_metadata_release_date,
            summary: String::new(),
            reading_direction: String::new(),
            total_book_count: None,
            alternate_titles: vec![],
            metadata_created: value.created.clone(),
            metadata_last_modified: value.last_modified.clone(),
            books_metadata_authors: vec![],
            books_metadata_tags: vec![],
            books_metadata_summary: String::new(),
            books_metadata_summary_number: String::new(),
            books_metadata_created: value.created,
            books_metadata_last_modified: value.last_modified,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxCollectionCandidateRow {
    id: String,
    name: String,
    ordered: bool,
    created_date: String,
    last_modified_date: String,
}

#[derive(sqlx::FromRow)]
struct SqlxCollectionSeriesRow {
    series_id: String,
}

#[derive(sqlx::FromRow)]
struct SqlxBookResourceRow {
    id: String,
    library_id: String,
    age_rating: Option<u16>,
    labels: String,
}

impl From<SqlxBookResourceRow> for BookResourceReadModel {
    fn from(value: SqlxBookResourceRow) -> Self {
        Self {
            id: value.id,
            library_id: value.library_id,
            age_rating: value.age_rating,
            labels: parse_labels(&value.labels),
        }
    }
}

pub(in crate::discovery) async fn list_libraries_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    let mut builder = QueryBuilder::<Sqlite>::new("SELECT id, name, root FROM libraries");
    let mut state = SqlxWhereState::default();
    if let Some(allowed_ids) = allowed.as_ref() {
        append_in_clause_sqlx("id", allowed_ids, &mut builder, &mut state);
    }
    builder.push(" ORDER BY name COLLATE NOCASE ASC");

    let rows = builder
        .build_query_as::<SqlxLibraryRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;

    Ok(rows.into_iter().map(LibraryReadModel::from).collect())
}

pub(super) async fn list_series_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeSeriesListQuery,
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

    let mut count_builder = QueryBuilder::<Sqlite>::new("SELECT COUNT(DISTINCT s.id) FROM series s");
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
        "SELECT s.id AS id, s.library_id AS library_id, s.title AS title, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS labels \
         FROM series s \
         LEFT JOIN series_labels sl ON sl.series_id = s.id",
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
        " GROUP BY s.id, s.library_id, s.title ORDER BY s.title COLLATE NOCASE ASC LIMIT ",
    );
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

pub(super) async fn get_series_detail_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &SeriesDetailQuery,
) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT \
            s.id AS id, s.library_id AS library_id, s.title AS title, s.url AS url, s.created AS created, s.last_modified AS last_modified, s.file_last_modified AS file_last_modified, \
            s.status AS status, s.publisher AS publisher, s.age_rating AS age_rating, s.language AS language, s.deleted AS deleted, s.oneshot AS oneshot, \
            COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS sharing_labels, \
            COALESCE(GROUP_CONCAT(DISTINCT sg.genre), '') AS genres, \
            COALESCE(GROUP_CONCAT(DISTINCT st.tag), '') AS tags, \
            COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id), 0) AS books_count, \
            COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id AND LOWER(b.read_status) = 'read'), 0) AS books_read_count, \
            COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id AND LOWER(b.read_status) = 'unread'), 0) AS books_unread_count, \
            COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id AND LOWER(b.read_status) = 'in_progress'), 0) AS books_in_progress_count, \
            COALESCE((SELECT MIN(b.metadata_release_date) FROM books b WHERE b.series_id = s.id), NULL) AS books_metadata_release_date \
         FROM series s \
         LEFT JOIN series_labels sl ON sl.series_id = s.id \
         LEFT JOIN series_genres sg ON sg.series_id = s.id \
         LEFT JOIN series_tags st ON st.series_id = s.id",
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

pub(in crate::discovery) async fn list_series_collections_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &SeriesCollectionsQuery,
) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    let mut candidate_builder = QueryBuilder::<Sqlite>::new(
        "SELECT DISTINCT c.id AS id, c.name AS name, c.ordered AS ordered, c.created_date AS created_date, c.last_modified_date AS last_modified_date \
         FROM collections c \
         JOIN collection_series cs_target ON cs_target.collection_id = c.id \
         JOIN series s ON s.id = cs_target.series_id",
    );
    let mut candidate_state = SqlxWhereState::default();
    apply_series_collection_visibility_sqlx(
        &mut candidate_builder,
        &mut candidate_state,
        allowed.as_ref(),
        context,
    );
    append_clause_sqlx("cs_target.series_id = ", &mut candidate_builder, &mut candidate_state);
    candidate_builder.push_bind(query.series_id.clone());
    candidate_state
        .params
        .push(SqlValue::Text(query.series_id.clone()));
    candidate_builder.push(" ORDER BY c.name COLLATE NOCASE ASC");

    let candidates = candidate_builder
        .build_query_as::<SqlxCollectionCandidateRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;

    let mut collections = vec![];
    for candidate in candidates {
        let mut visible_builder = QueryBuilder::<Sqlite>::new(
            "SELECT cs.series_id AS series_id \
             FROM collection_series cs \
             JOIN series s ON s.id = cs.series_id",
        );
        let mut visible_state = SqlxWhereState::default();
        apply_series_collection_visibility_sqlx(
            &mut visible_builder,
            &mut visible_state,
            allowed.as_ref(),
            context,
        );
        append_clause_sqlx("cs.collection_id = ", &mut visible_builder, &mut visible_state);
        visible_builder.push_bind(candidate.id.clone());
        visible_state.params.push(SqlValue::Text(candidate.id.clone()));
        visible_builder.push(" ORDER BY cs.position ASC");

        let visible_rows = visible_builder
            .build_query_as::<SqlxCollectionSeriesRow>()
            .fetch_all(&pool)
            .await
            .map_err(map_sqlx_error)?;
        let visible_series_ids = visible_rows
            .into_iter()
            .map(|row| row.series_id)
            .collect::<Vec<_>>();

        if visible_series_ids.is_empty() {
            continue;
        }

        let total_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM collection_series WHERE collection_id = ?")
                .bind(candidate.id.clone())
                .fetch_one(&pool)
                .await
                .map_err(map_sqlx_error)?;

        collections.push(CollectionReadModel {
            id: candidate.id,
            name: candidate.name,
            ordered: candidate.ordered,
            series_ids: visible_series_ids.clone(),
            created_date: candidate.created_date,
            last_modified_date: candidate.last_modified_date,
            filtered: (visible_series_ids.len() as i64) < total_count,
        });
    }

    Ok(collections)
}

pub(in crate::discovery) async fn resolve_series_resource_sqlx(
    pool: SqlitePool,
    series_id: &str,
) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
    let row = sqlx::query_as::<_, SqlxSeriesResourceRow>(
        "SELECT s.id, s.library_id, s.age_rating, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS labels \
         FROM series s \
         LEFT JOIN series_labels sl ON sl.series_id = s.id \
         WHERE s.id = ? \
         GROUP BY s.id, s.library_id, s.age_rating",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(row.map(SeriesResourceReadModel::from))
}

pub(in crate::discovery) async fn resolve_book_resource_sqlx(
    pool: SqlitePool,
    book_id: &str,
) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
    let row = sqlx::query_as::<_, SqlxBookResourceRow>(
        "SELECT b.id, b.library_id, s.age_rating, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS labels \
         FROM books b \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN series_labels sl ON sl.series_id = s.id \
         WHERE b.id = ? \
         GROUP BY b.id, b.library_id, s.age_rating",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(row.map(BookResourceReadModel::from))
}

fn apply_series_list_filters_sqlx<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    context: &DiscoveryQueryContext,
    query: &NativeSeriesListQuery,
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

fn apply_series_collection_visibility_sqlx<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    allowed_library_ids: Option<&Vec<String>>,
    context: &DiscoveryQueryContext,
) {
    if let Some(allowed_ids) = allowed_library_ids {
        append_in_clause_sqlx("s.library_id", allowed_ids, builder, state);
    }
    if let Some(restrictions) = context.restrictions.as_ref() {
        apply_restrictions_sqlx("s", restrictions, builder, state);
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
    state.params.push(SqlValue::Integer(i64::from(value)));
}

pub(super) fn map_sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::Persistence(error.to_string())
}

pub(super) fn parse_labels(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
}
