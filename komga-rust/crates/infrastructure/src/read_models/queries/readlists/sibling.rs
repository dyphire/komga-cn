use komga_application::discovery::BookDetailReadModel;
use komga_application::discovery::{BookDetailQuery, RuntimeReadListBooksQuery};
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use sqlx::SqlitePool;

use super::super::book_detail::get_book_detail_sqlx;
use super::list_readlist_books_sqlx;

pub(super) async fn get_readlist_book_sibling_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
    book_id: &str,
    next: bool,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    let page = list_readlist_books_sqlx(
        pool.clone(),
        context,
        &RuntimeReadListBooksQuery {
            readlist_id: readlist_id.to_string(),
            page: 0,
            size: 20,
            unpaged: true,
            library_ids: None,
            deleted: None,
            tags: None,
            read_statuses: None,
            media_statuses: None,
            authors: None,
        },
    )
    .await?;
    let visible_book_ids = page
        .content
        .iter()
        .map(|it| it.id.as_str())
        .collect::<Vec<_>>();

    let Some(current_index) = visible_book_ids.iter().position(|id| *id == book_id) else {
        return Ok(None);
    };

    let sibling_id = if next {
        visible_book_ids.get(current_index + 1)
    } else if current_index == 0 {
        None
    } else {
        visible_book_ids.get(current_index - 1)
    };

    let Some(sibling_id) = sibling_id else {
        return Ok(None);
    };

    get_book_detail_sqlx(
        pool,
        context,
        &BookDetailQuery {
            book_id: (*sibling_id).to_string(),
        },
    )
    .await
}
