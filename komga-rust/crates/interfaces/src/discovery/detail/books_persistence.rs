use komga_application::discovery::{BookReadModel, PersistedBookSiblingDirectionRecord};

use super::detail_utils::parse_group_concat_values;
use crate::state::DiscoveryState;

#[derive(Clone)]
pub(in crate::discovery) struct PersistedBookResource {
    pub(in crate::discovery) library_id: String,
    pub(in crate::discovery) age_rating: Option<u32>,
    pub(in crate::discovery) sharing_labels: Vec<String>,
}

#[derive(Clone, Copy)]
pub(super) enum PersistedBookSiblingDirection {
    Previous,
    Next,
}

pub(in crate::discovery) async fn load_persisted_book_resource(
    app: &DiscoveryState,
    book_id: &str,
) -> Result<Option<PersistedBookResource>, String> {
    let resource = app
        .book_detail
        .load_persisted_book_resource(book_id)
        .await?
        .map(|row| PersistedBookResource {
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: parse_group_concat_values(&row.sharing_labels),
        });
    Ok(resource)
}

pub(super) async fn load_persisted_book_detail(
    app: &DiscoveryState,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<BookReadModel>, String> {
    app.book_detail
        .load_persisted_book_detail(book_id, user_id)
        .await
}

pub(super) async fn load_persisted_book_sibling_detail(
    app: &DiscoveryState,
    book_id: &str,
    direction: PersistedBookSiblingDirection,
    user_id: Option<&str>,
) -> Result<Option<BookReadModel>, String> {
    let direction = match direction {
        PersistedBookSiblingDirection::Previous => PersistedBookSiblingDirectionRecord::Previous,
        PersistedBookSiblingDirection::Next => PersistedBookSiblingDirectionRecord::Next,
    };

    let Some(sibling_id) = app
        .book_detail
        .load_persisted_book_sibling_id(book_id, direction)
        .await?
    else {
        return Ok(None);
    };

    load_persisted_book_detail(app, &sibling_id, user_id).await
}
