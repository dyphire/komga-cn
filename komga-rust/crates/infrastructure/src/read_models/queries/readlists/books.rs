use komga_application::discovery::BookReadModel;
use komga_application::discovery::RuntimeReadListBooksQuery;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, PageEnvelope};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::super::books::apply_books_filters_sqlx;
use super::{SqlxReadlistBookRow, browse_page_size, map_sqlx_error, readlist_book_order_sql};
use crate::read_models::filters::{SqlxWhereState, append_clause_sqlx, effective_library_ids};

pub(super) async fn list_readlist_books_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    query: &RuntimeReadListBooksQuery,
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

    let ordered = sqlx::query_scalar::<_, bool>(
        r#"
            SELECT ordered
            FROM readlists
            WHERE id = ?
        "#,

    )
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
        r#"
            SELECT COUNT(DISTINCT b.id)
            FROM readlist_books rlb
            JOIN books b ON b.id = rlb.book_id
            JOIN series s ON s.id = b.series_id
        "#,

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
        r#"
            SELECT b.id AS id,
                   b.series_id AS series_id,
                   b.library_id AS library_id,
                   b.title AS title,
                   b.url AS url,
                   CAST(b.number_sort AS INTEGER) AS number,
                   b.created AS created,
                   b.last_modified AS last_modified,
                   b.file_last_modified AS file_last_modified,
                   b.size_bytes AS size_bytes,
                   COALESCE(b.media_status, 'UNKNOWN') AS media_status,
                   COALESCE(b.media_type, '') AS media_type,
                   COALESCE(b.media_pages_count, 0) AS media_pages_count,
                   '' AS media_comment,
                   0 AS media_epub_divina_compatible,
                   0 AS media_epub_is_kepub,
                   b.metadata_release_date AS metadata_release_date,
                   0 AS metadata_title_lock,
                   '' AS metadata_summary,
                   0 AS metadata_summary_lock,
                   CAST(b.number_sort AS TEXT) AS metadata_number,
                   0 AS metadata_number_lock,
                   CAST(b.number_sort AS REAL) AS metadata_number_sort,
                   0 AS metadata_number_sort_lock,
                   0 AS metadata_release_date_lock,
                   COALESCE((SELECT GROUP_CONCAT(ba.author, X'1F') FROM book_authors ba WHERE ba.book_id = b.id), '') AS metadata_authors,
                   0 AS metadata_authors_lock,
                   COALESCE((SELECT GROUP_CONCAT(bt.tag) FROM book_tags bt WHERE bt.book_id = b.id), '') AS metadata_tags,
                   0 AS metadata_tags_lock,
                   '' AS metadata_isbn,
                   0 AS metadata_isbn_lock,
                   '' AS metadata_links,
                   0 AS metadata_links_lock,
                   b.created AS metadata_created,
                   b.last_modified AS metadata_last_modified,
                   rp.page AS read_progress_page,
                   rp.completed AS read_progress_completed,
                   rp.read_date AS read_progress_read_date,
                   rp.created AS read_progress_created,
                   rp.last_modified AS read_progress_last_modified,
                   rp.device_id AS read_progress_device_id,
                   rp.device_name AS read_progress_device_name,
                   b.deleted AS deleted,
                   '' AS file_hash,
                   b.oneshot AS oneshot,
                   s.title AS series_title
            FROM readlist_books rlb
            JOIN books b ON b.id = rlb.book_id
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
    select_builder.push(" ORDER BY ");
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
