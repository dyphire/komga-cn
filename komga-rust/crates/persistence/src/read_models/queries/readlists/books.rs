use komga_application::discovery::NativeReadListBooksQuery;
use komga_domain::discovery::{BookReadModel, DiscoveryError, DiscoveryQueryContext, PageEnvelope};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::super::books::apply_books_filters_sqlx;
use super::{SqlxReadlistBookRow, browse_page_size, map_sqlx_error, readlist_book_order_sql};
use crate::read_models::filters::{SqlxWhereState, append_clause_sqlx, effective_library_ids};

pub(super) async fn list_readlist_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &NativeReadListBooksQuery,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, query.library_ids.as_deref());
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(
            vec![],
            query.page,
            browse_page_size(query.size),
            0,
        ));
    }

    let ordered = sqlx::query_scalar::<_, bool>("SELECT ordered FROM readlists WHERE id = ?")
        .bind(&query.readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;
    let Some(ordered) = ordered else {
        return Ok(PageEnvelope::from_slice(
            vec![],
            query.page,
            browse_page_size(query.size),
            0,
        ));
    };

    let mut count_builder = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(DISTINCT b.id) FROM readlist_books rlb JOIN books b ON b.id = rlb.book_id JOIN series s ON s.id = b.series_id",
    );
    let mut count_state = SqlxWhereState::default();
    apply_books_filters_sqlx(
        &mut count_builder,
        &mut count_state,
        context,
        allowed.as_ref(),
        None,
        query.deleted,
        None,
        true,
        query.tags.as_deref(),
        query.read_statuses.as_deref(),
        None,
        query.media_statuses.as_deref(),
        query.authors.as_deref(),
        None,
        None,
    );
    append_clause_sqlx("rlb.readlist_id = ", &mut count_builder, &mut count_state);
    count_builder.push_bind(query.readlist_id.clone());
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
         FROM readlist_books rlb \
         JOIN books b ON b.id = rlb.book_id \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN series_labels sl ON sl.series_id = s.id",
    );
    let mut select_state = SqlxWhereState::default();
    apply_books_filters_sqlx(
        &mut select_builder,
        &mut select_state,
        context,
        allowed.as_ref(),
        None,
        query.deleted,
        None,
        true,
        query.tags.as_deref(),
        query.read_statuses.as_deref(),
        None,
        query.media_statuses.as_deref(),
        query.authors.as_deref(),
        None,
        None,
    );
    append_clause_sqlx("rlb.readlist_id = ", &mut select_builder, &mut select_state);
    select_builder.push_bind(query.readlist_id.clone());
    select_builder.push(
        " GROUP BY b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, \
            b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title \
          ORDER BY ",
    );
    select_builder.push(readlist_book_order_sql(ordered));

    let (envelope_page, envelope_size) = if query.unpaged {
        (0, total_elements.max(1))
    } else {
        let safe_size = browse_page_size(query.size);
        let offset = query.page.saturating_mul(safe_size);
        select_builder.push(" LIMIT ");
        select_builder.push_bind(safe_size as i64);
        select_builder.push(" OFFSET ");
        select_builder.push_bind(offset as i64);
        (query.page, safe_size)
    };

    let rows = select_builder
        .build_query_as::<SqlxReadlistBookRow>()
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
