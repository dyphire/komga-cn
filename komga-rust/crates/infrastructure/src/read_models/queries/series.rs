use komga_application::discovery::SeriesBrowseQuery;
use komga_application::discovery::SeriesReadModel;
use komga_domain::discovery::{
    AgeRatingCondition, CompositeSeriesCondition, DateCondition, DiscoveryError,
    DiscoveryQueryContext, FilterOperator, InclusionCondition, PageEnvelope, ReadStatusCondition,
    SeriesCondition, SeriesSort, SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::super::filters::{
    SqlxWhereState, append_bool_sqlx_filter, append_comparison_sqlx, append_in_clause_sqlx,
    append_like_clause_sqlx, append_not_in_clause_sqlx, append_subquery_exists_clause,
    effective_library_ids, query_filters_sqlx,
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
struct SqlxSeriesListRow {
    id: String,
    title: String,
}

impl From<SqlxSeriesListRow> for SeriesReadModel {
    fn from(value: SqlxSeriesListRow) -> Self {
        Self {
            id: value.id,
            name: value.title.clone(),
            title: value.title,
        }
    }
}

pub(in crate::read_models) async fn list_series_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &SeriesBrowseQuery,
) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(
            vec![],
            query.page,
            query.size.max(1),
            0,
        ));
    }

    let mut count_builder = QueryBuilder::<Sqlite>::new(
        r#"
            SELECT COUNT(DISTINCT s.id)
            FROM series s
        "#,
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
        r#"
            SELECT s.id AS id,
                   s.library_id AS library_id,
                   s.title AS title,
                   s.created AS created,
                   s.last_modified AS last_modified,
                   s.release_date AS release_date,
                   s.book_count AS book_count,
                   COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') AS labels
            FROM series s
            LEFT JOIN series_labels sl ON sl.series_id = s.id
        "#,
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
        r#"
            GROUP BY s.id, s.library_id, s.title, s.created, s.last_modified, s.release_date, s.book_count ORDER BY
        "#,

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

fn series_ordering_from_sorts(sorts: &[SeriesSort]) -> SeriesOrdering {
    match sorts.first() {
        Some(SeriesSort::MetadataTitleSortAsc) | None => SeriesOrdering::TitleAsc,
        Some(SeriesSort::CreatedDateDesc) => SeriesOrdering::CreatedDateDesc,
        Some(SeriesSort::LastModifiedDateDesc) => SeriesOrdering::LastModifiedDateDesc,
        Some(SeriesSort::ReleaseDateDesc) => SeriesOrdering::BooksMetadataReleaseDateDesc,
        Some(SeriesSort::BooksCountDesc) => SeriesOrdering::BooksCountDesc,
        _ => SeriesOrdering::TitleAsc,
    }
}

fn series_order_sql(ordering: SeriesOrdering) -> &'static str {
    match ordering {
        SeriesOrdering::TitleAsc => "s.title COLLATE NOCASE ASC",
        SeriesOrdering::CreatedDateDesc => "s.created DESC, s.title COLLATE NOCASE ASC",
        SeriesOrdering::LastModifiedDateDesc => "s.last_modified DESC, s.title COLLATE NOCASE ASC",
        SeriesOrdering::BooksMetadataReleaseDateDesc => {
            "s.release_date DESC, s.title COLLATE NOCASE ASC"
        }
        SeriesOrdering::BooksCountDesc => "s.book_count DESC, s.title COLLATE NOCASE ASC",
    }
}

fn apply_series_list_filters_sqlx<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    context: &DiscoveryQueryContext,
    query: &SeriesBrowseQuery,
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

    if let Some(condition) = &query.filter.condition {
        apply_series_condition_sqlx(condition, builder, state);
    }
}

fn apply_series_condition_sqlx<'args>(
    condition: &SeriesCondition,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    match condition {
        SeriesCondition::Value(value) => apply_series_value_condition_sqlx(value, builder, state),
        SeriesCondition::Composite(CompositeSeriesCondition {
            operator,
            conditions,
        }) => match operator {
            FilterOperator::All => {
                for c in conditions {
                    apply_series_condition_sqlx(c, builder, state);
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
                    apply_series_condition_sqlx(c, builder, state);
                }
                builder.push(")");
                state.has_where = true;
            }
        },
    }
}

fn apply_series_value_condition_sqlx<'args>(
    value: &SeriesValueCondition,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    match value {
        SeriesValueCondition::Deleted(val) => {
            append_bool_sqlx_filter("s.deleted", *val, builder, state);
        }
        SeriesValueCondition::OneShot(val) => {
            append_bool_sqlx_filter("s.oneshot", *val, builder, state);
        }
        SeriesValueCondition::Complete(val) => {
            append_bool_sqlx_filter("s.complete", *val, builder, state);
        }
        SeriesValueCondition::LibraryId(InclusionCondition::Include(ids)) => {
            let str_ids: Vec<String> = ids.iter().map(|id| id.as_str().to_string()).collect();
            append_in_clause_sqlx("s.library_id", &str_ids, builder, state);
        }
        SeriesValueCondition::LibraryId(InclusionCondition::Exclude(ids)) => {
            for id in ids {
                append_not_in_clause_sqlx("s.library_id", id.as_str(), builder, state);
            }
        }
        SeriesValueCondition::Title(StringCondition::Contains(InclusionCondition::Include(v))) => {
            for val in v {
                append_like_clause_sqlx("s.title", &format!("%{val}%"), builder, state);
            }
        }
        SeriesValueCondition::Title(StringCondition::Exact(InclusionCondition::Include(v))) => {
            append_in_clause_sqlx("s.title", v, builder, state);
        }
        SeriesValueCondition::Title(StringCondition::StartsWith(InclusionCondition::Include(
            v,
        ))) => {
            for val in v {
                append_like_clause_sqlx("s.title", &format!("{val}%"), builder, state);
            }
        }
        SeriesValueCondition::Title(StringCondition::EndsWith(InclusionCondition::Include(v))) => {
            for val in v {
                append_like_clause_sqlx("s.title", &format!("%{val}"), builder, state);
            }
        }
        SeriesValueCondition::Genre(StringCondition::Exact(InclusionCondition::Include(v)))
        | SeriesValueCondition::Genre(StringCondition::Contains(InclusionCondition::Include(v))) => {
            append_subquery_exists_clause("series_genres", "series_id", "genre", v, builder, state);
        }
        SeriesValueCondition::Tag(StringCondition::Exact(InclusionCondition::Include(v)))
        | SeriesValueCondition::Tag(StringCondition::Contains(InclusionCondition::Include(v))) => {
            append_subquery_exists_clause("series_tags", "series_id", "tag", v, builder, state);
        }
        SeriesValueCondition::Language(InclusionCondition::Include(v)) => {
            append_in_clause_sqlx("s.language", v, builder, state);
        }
        SeriesValueCondition::Publisher(InclusionCondition::Include(v)) => {
            append_in_clause_sqlx("s.publisher", v, builder, state);
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::Exact(
            InclusionCondition::Include(v),
        )) => {
            let str_vals: Vec<String> = v.iter().map(|r| r.to_string()).collect();
            append_in_clause_sqlx("s.age_rating", &str_vals, builder, state);
        }
        SeriesValueCondition::ReadStatus(ReadStatusCondition::Include(v)) => {
            append_in_clause_sqlx("s.read_status", v, builder, state);
        }
        SeriesValueCondition::SeriesStatus(SeriesStatusCondition::Include(v)) => {
            append_in_clause_sqlx("s.status", v, builder, state);
        }
        SeriesValueCondition::ReleaseDate(DateCondition::After(date)) => {
            append_comparison_sqlx("s.release_date", ">", date, builder, state);
        }
        SeriesValueCondition::ReleaseDate(DateCondition::Before(date)) => {
            append_comparison_sqlx("s.release_date", "<", date, builder, state);
        }
        SeriesValueCondition::ReleaseDate(DateCondition::Exact(InclusionCondition::Include(v))) => {
            append_in_clause_sqlx("s.release_date", v, builder, state);
        }
        SeriesValueCondition::SharingLabel(StringCondition::Exact(
            InclusionCondition::Include(v),
        ))
        | SeriesValueCondition::SharingLabel(StringCondition::Contains(
            InclusionCondition::Include(v),
        )) => {
            append_subquery_exists_clause("series_labels", "series_id", "label", v, builder, state);
        }
        SeriesValueCondition::Author(StringCondition::Contains(InclusionCondition::Include(v)))
        | SeriesValueCondition::Author(StringCondition::Exact(InclusionCondition::Include(v))) => {
            append_subquery_exists_clause(
                "series_authors",
                "series_id",
                "author",
                v,
                builder,
                state,
            );
        }
        // Ignore unsupported/exclude variants
        _ => {}
    }
}
