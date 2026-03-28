use komga_application::discovery::NativeReadListsQuery;
use komga_domain::discovery::{
    DiscoveryError, DiscoveryQueryContext, PageEnvelope, ReadListReadModel,
};
use sqlx::SqlitePool;

use super::{
    browse_page_size, list_readlist_candidate_rows_sqlx, map_sqlx_error,
    visible_readlist_book_ids_sqlx,
};
use crate::read_models::filters::effective_library_ids;

pub(super) async fn list_readlists_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeReadListsQuery,
) -> Result<PageEnvelope<ReadListReadModel>, DiscoveryError> {
    let candidate_library_ids = effective_library_ids(context, query.library_ids.as_deref());
    if candidate_library_ids.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(
            vec![],
            query.page,
            browse_page_size(query.size),
            0,
        ));
    }

    let candidate_rows = list_readlist_candidate_rows_sqlx(
        pool.clone(),
        context,
        candidate_library_ids.as_ref(),
        query.search.as_deref(),
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

        let total_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) \
             FROM readlist_books \
             WHERE readlist_id = ?",
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

    Ok(PageEnvelope::from_slice(
        readlists,
        query.page,
        page_size,
        total_elements,
    ))
}
