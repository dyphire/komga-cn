use komga_application::discovery::{BookReadlistsQuery, ReadListDetailQuery};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, ReadListReadModel};
use sqlx::SqlitePool;

use super::{SqlxReadListCandidateRow, map_sqlx_error, visible_readlist_book_ids_sqlx};
use crate::read_models::filters::effective_library_ids;

pub(super) async fn list_book_readlists_sqlx(
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
        let visible_book_ids =
            visible_readlist_book_ids_sqlx(pool.clone(), context, &candidate.id, allowed.as_ref())
                .await?;
        if visible_book_ids.is_empty() {
            continue;
        }

        let total_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM readlist_books WHERE readlist_id = ?",
        )
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

pub(super) async fn get_readlist_detail_sqlx(
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

    let visible_book_ids =
        visible_readlist_book_ids_sqlx(pool.clone(), context, &candidate.id, allowed.as_ref())
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
