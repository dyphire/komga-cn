use komga_application::discovery::CollectionReadModel;
use komga_application::discovery::SeriesCollectionsQuery;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::map_sqlx_error;
use crate::read_models::filters::{
    SqlxWhereState, append_clause_sqlx, append_in_clause_sqlx, apply_restrictions_sqlx,
    effective_library_ids,
};

#[derive(sqlx::FromRow)]
struct SqlxCollectionCandidateRow {
    id: String,
    name: String,
}

#[derive(sqlx::FromRow)]
struct SqlxCollectionSeriesRow {
    series_id: String,
}

pub(super) async fn list_series_collections_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &SeriesCollectionsQuery,
) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    let mut candidate_builder = QueryBuilder::<Sqlite>::new(
        r#"
            SELECT DISTINCT c.id AS id,
                            c.name AS name,
                            c.ordered AS ordered,
                            c.created_date AS created_date,
                            c.last_modified_date AS last_modified_date
            FROM collections c
            JOIN collection_series cs_target ON cs_target.collection_id = c.id
            JOIN series s ON s.id = cs_target.series_id
        "#,

    );
    let mut candidate_state = SqlxWhereState::default();
    apply_series_collection_visibility_sqlx(
        &mut candidate_builder,
        &mut candidate_state,
        allowed.as_ref(),
        context,
    );
    append_clause_sqlx(
        "cs_target.series_id = ",
        &mut candidate_builder,
        &mut candidate_state,
    );
    candidate_builder.push_bind(query.series_id.clone());
    candidate_builder.push(" ORDER BY c.name COLLATE NOCASE ASC");

    let candidates = candidate_builder
        .build_query_as::<SqlxCollectionCandidateRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;

    let mut collections = vec![];
    for candidate in candidates {
        let mut visible_builder = QueryBuilder::<Sqlite>::new(
        r#"
            SELECT cs.series_id AS series_id
            FROM collection_series cs
            JOIN series s ON s.id = cs.series_id
        "#,

        );
        let mut visible_state = SqlxWhereState::default();
        apply_series_collection_visibility_sqlx(
            &mut visible_builder,
            &mut visible_state,
            allowed.as_ref(),
            context,
        );
        append_clause_sqlx(
            "cs.collection_id = ",
            &mut visible_builder,
            &mut visible_state,
        );
        visible_builder.push_bind(candidate.id.clone());
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

        let _total_count = sqlx::query_scalar::<_, i64>(
        r#"
            SELECT COUNT(*)
            FROM collection_series
            WHERE collection_id = ?
        "#,

        )
        .bind(candidate.id.clone())
        .fetch_one(&pool)
        .await
        .map_err(map_sqlx_error)?;

        collections.push(CollectionReadModel {
            id: candidate.id,
            name: candidate.name,
        });
    }

    Ok(collections)
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
